// Keykeeper: joining a client organization through the RMM install command.
// The command gives the browser extension the client's enrollment token. After
// the person signs in and unlocks, the extension asks which organization the
// token belongs to, encrypts the person's user key to that organization's
// public key for account recovery (upstream's own enrollment code), and joins.
//
// A join goes through upstream's invite acceptance, so it gets exactly the
// checks an emailed invite gets: single organization in both directions,
// two-step login, and the account recovery key. It never includes the org
// key: the member waits to be confirmed, which the MSP console does. A client
// can limit the command to accounts at its email domains, and its own owners
// get an email about each join. See DESIGN.md, "Onboarding through the RMM".

use rocket::{Route, serde::json::Json};

use super::keykeeper_msp::{Audit, require_admin, staff_member};
use crate::{
    CONFIG,
    api::JsonResult,
    auth::Headers,
    db::{
        DbConn,
        models::{
            CollectionUser, EnrollmentDomains, Membership, MembershipStatus, MembershipType, MspClient,
            MspEnrollmentJoin, MspId, MspMembership, MspRole, OrgPolicy, Organization, OrganizationId, User,
            effective_joining, email_in_domains, parse_enrollment_domains,
        },
    },
    mail,
};

pub fn routes() -> Vec<Route> {
    routes![get_enrollment, replace_enrollment, set_enrollment_domains, enrollment_info, join]
}

const INVALID_TOKEN: &str = "This install link isn't valid anymore. Ask your IT provider for a new one.";

/// One of this MSP's clients, or "not found".
async fn msp_client(msp_id: &MspId, org_id: &OrganizationId, conn: &DbConn) -> Result<MspClient, crate::error::Error> {
    match MspClient::find_by_org(org_id, conn).await {
        Some(client) if &client.msp_uuid == msp_id => Ok(client),
        _ => err_code!("Client not found", rocket::http::Status::NotFound.code),
    }
}

async fn enrollment_json(client: &MspClient, conn: &DbConn) -> serde_json::Value {
    json!({
        "token": client.enrollment_token,
        "createdAt": client.enrollment_token_created_at.as_ref().map(crate::util::format_date),
        "joins": MspEnrollmentJoin::count_by_org(&client.org_uuid, conn).await,
        "domains": EnrollmentDomains::for_org(&client.org_uuid, conn).await,
    })
}

/// A client's enrollment token, for its install command. MSP admins, and
/// technicians who reach the client and are allowed install commands.
#[get("/keykeeper/msp/<msp_id>/clients/<org_id>/enrollment")]
async fn get_enrollment(msp_id: MspId, org_id: OrganizationId, headers: Headers, conn: DbConn) -> JsonResult {
    let staff = staff_member(&msp_id, &headers.user.uuid, &conn).await?;
    let client = msp_client(&msp_id, &org_id, &conn).await?;
    if staff.role() == Some(MspRole::Technician) {
        if staff.reach(&conn).await.is_some_and(|r| !r.contains(&org_id)) {
            err_code!("Client not found", rocket::http::Status::NotFound.code)
        }
        if !staff.install_commands {
            err_code!("Your MSP's admins haven't given you install commands", rocket::http::Status::Forbidden.code)
        }
    }
    Ok(Json(enrollment_json(&client, &conn).await))
}

/// Replaces a client's enrollment token. MSP admins only: install commands with
/// the old token stop letting new people join.
#[post("/keykeeper/msp/<msp_id>/clients/<org_id>/enrollment")]
async fn replace_enrollment(msp_id: MspId, org_id: OrganizationId, headers: Headers, conn: DbConn) -> JsonResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let mut client = msp_client(&msp_id, &org_id, &conn).await?;
    client.replace_enrollment_token(&conn).await?;
    let org_name = Organization::find_by_uuid(&org_id, &conn).await.map(|o| o.name).unwrap_or_default();
    Audit::by("install_token_replaced", &headers).org(&org_id, &org_name).record(&msp_id, &conn).await?;
    Ok(Json(enrollment_json(&client, &conn).await))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DomainsData {
    domains: Vec<String>,
}

/// Limits a client's install command to accounts at these email domains, or
/// with none, lets anyone with the command join again. MSP admins only. The
/// limit trusts an account's email only once it's verified, so it needs a
/// server that sends email.
#[put("/keykeeper/msp/<msp_id>/clients/<org_id>/enrollment/domains", data = "<data>")]
async fn set_enrollment_domains(
    msp_id: MspId,
    org_id: OrganizationId,
    data: Json<DomainsData>,
    headers: Headers,
    conn: DbConn,
) -> JsonResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let client = msp_client(&msp_id, &org_id, &conn).await?;
    let domains = match parse_enrollment_domains(&data.domains) {
        Ok(domains) => domains,
        Err(e) => err!(e),
    };
    if !domains.is_empty() && !CONFIG.mail_enabled() {
        err!("Limiting the install command to email domains needs verified emails, and this server can't send email")
    }
    EnrollmentDomains::set(&org_id, &domains, &conn).await?;
    let org_name = Organization::find_by_uuid(&org_id, &conn).await.map(|o| o.name).unwrap_or_default();
    Audit::by("install_domains_set", &headers)
        .org(&org_id, &org_name)
        .detail(domains.join(" "))
        .record(&msp_id, &conn)
        .await?;
    Ok(Json(enrollment_json(&client, &conn).await))
}

/// Refuses an account outside a client's email domains, or one whose email
/// isn't verified, when the client has domains. The message is what the
/// person sees in the extension.
fn check_domains(user: &User, org: &Organization, domains: &[String]) -> crate::api::EmptyResult {
    if domains.is_empty() {
        return Ok(());
    }
    let listed = match domains {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        [] => unreachable!(),
    };
    if !email_in_domains(&user.email, domains) {
        err!(format!(
            "{}'s install link is for {listed} accounts, and you're signed in as {}. Ask its admins to invite you.",
            org.name, user.email
        ))
    }
    if user.verified_at.is_none() {
        err!(format!(
            "Verify your email address first: {}'s install link is for verified {listed} accounts. Settings, then My account, sends the email.",
            org.name
        ))
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnrollmentTokenData {
    token: String,
}

/// Which organization a token joins, and what's needed to join it. The
/// public key is for account recovery enrollment.
#[post("/keykeeper/enrollment/info", data = "<data>")]
async fn enrollment_info(data: Json<EnrollmentTokenData>, headers: Headers, conn: DbConn) -> JsonResult {
    let Some(client) = MspClient::find_by_enrollment_token(&data.token, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
    let Some(org) = Organization::find_by_uuid(&client.org_uuid, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
    // Said here too, so the extension says why before it does any work.
    let invited = Membership::find_by_user_and_org(&headers.user.uuid, &org.uuid, &conn).await.is_some();
    if !invited {
        check_domains(&headers.user, &org, &EnrollmentDomains::for_org(&org.uuid, &conn).await)?;
    }
    Ok(Json(json!({
        "organizationId": org.uuid,
        "organizationName": org.name,
        "publicKey": org.public_key,
        "accountRecovery": OrgPolicy::org_is_reset_password_auto_enroll(&org.uuid, &conn).await,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JoinData {
    token: String,
    /// The person's user key encrypted to the organization's public key, when
    /// the organization enrolls members in account recovery automatically.
    reset_password_key: Option<String>,
}

/// Joins the organization a token belongs to, as a regular member waiting to
/// be confirmed. Joining again is harmless.
#[post("/keykeeper/enrollment/join", data = "<data>")]
async fn join(data: Json<JoinData>, headers: Headers, conn: DbConn) -> JsonResult {
    let data = data.into_inner();
    let Some(client) = MspClient::find_by_enrollment_token(&data.token, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
    let Some(org) = Organization::find_by_uuid(&client.org_uuid, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
    let joined =
        |status: i32| Json(json!({ "organizationId": org.uuid, "organizationName": org.name, "status": status }));

    let existing = Membership::find_by_user_and_org(&headers.user.uuid, &org.uuid, &conn).await;
    let member = match existing {
        // Already in: nothing to do.
        Some(member) if member.status >= MembershipStatus::Accepted as i32 => return Ok(joined(member.status)),
        Some(member) if member.status < MembershipStatus::Invited as i32 => {
            err!(format!("Your access to {} was turned off. Ask its admins.", org.name))
        }
        // Invited by email, joining through the install command instead. An
        // admin chose them, so the client's domains don't apply.
        Some(member) => member,
        None => {
            check_domains(&headers.user, &org, &EnrollmentDomains::for_org(&org.uuid, &conn).await)?;
            let mut member = Membership::new(headers.user.uuid.clone(), org.uuid.clone(), None);
            member.status = MembershipStatus::Invited as i32;
            member.atype = MembershipType::User as i32;
            member.access_all = false;
            member
        }
    };

    let reset_password_key = match OrgPolicy::org_is_reset_password_auto_enroll(&org.uuid, &conn).await {
        true if data.reset_password_key.as_deref().is_none_or(str::is_empty) => {
            err!(format!("Joining {} needs account recovery enrollment", org.name))
        }
        true => data.reset_password_key,
        false => None,
    };

    let member_id = member.uuid.clone();
    let invited_by = member.invited_by_email.clone();
    super::accept_org_invite(&headers.user, member, reset_password_key, false, &conn).await?;

    if let Some(collection) = &client.everyone_collection {
        CollectionUser::save(&headers.user.uuid, collection, false, false, false, &conn).await?;
    }
    MspEnrollmentJoin::new(member_id, org.uuid.clone()).insert(&conn).await?;
    Audit::by("joined_with_install_command", &headers)
        .org(&org.uuid, &org.name)
        .target(&headers.user.email)
        .record(&client.msp_uuid, &conn)
        .await?;
    tell_owners(&headers.user, &org, invited_by, &conn).await;

    Ok(joined(MembershipStatus::Accepted as i32))
}

/// Emails a client's own owners that someone joined with the install command:
/// with nobody clicking Confirm by default, this is how they hear of it. Its
/// MSP's people aren't emailed, since a rollout would mean one email per
/// machine; the console's Activity has every join. Someone who was invited by
/// email is the inviter's to hear about, as for any invitation.
async fn tell_owners(user: &User, org: &Organization, invited_by: Option<String>, conn: &DbConn) {
    if !CONFIG.mail_enabled() {
        return;
    }
    let approve = !effective_joining(&org.uuid, conn).await.0.auto_confirm;
    let mut to: Vec<String> = Vec::new();
    if let Some(inviter) = invited_by {
        to.push(inviter);
    } else {
        for owner in Membership::find_by_org_and_type(&org.uuid, MembershipType::Owner, conn).await {
            if owner.status != MembershipStatus::Confirmed as i32
                || owner.user_uuid == user.uuid
                || MspMembership::find_by_membership(&owner.uuid, conn).await.is_some()
            {
                continue;
            }
            if let Some(owner) = User::find_by_uuid(&owner.user_uuid, conn).await {
                to.push(owner.email);
            }
        }
    }
    for address in to {
        if let Err(e) = mail::send_member_joined(&address, &user.email, &org.name, &org.uuid, true, approve).await {
            error!("Error sending the join email for {}: {e:#?}", org.name);
        }
    }
}

// Keykeeper: joining a client organization through the RMM install command.
// The command gives the browser extension the client's enrollment token. After
// the person signs in and unlocks, the extension asks which organization the
// token belongs to, encrypts the person's user key to that organization's
// public key for account recovery (upstream's own enrollment code), and joins.
//
// A join goes through upstream's invite acceptance, so it gets exactly the
// checks an emailed invite gets: single organization in both directions,
// two-step login, and the account recovery key. It never includes the org
// key: the member waits to be confirmed, which the MSP console does. See
// DESIGN.md, "Onboarding through the RMM".

use rocket::{Route, serde::json::Json};

use super::keykeeper_msp::{Audit, require_admin, staff_role};
use crate::{
    api::JsonResult,
    auth::Headers,
    db::{
        DbConn,
        models::{
            CollectionUser, Membership, MembershipStatus, MembershipType, MspClient, MspEnrollmentJoin, MspId,
            OrgPolicy, Organization, OrganizationId,
        },
    },
};

pub fn routes() -> Vec<Route> {
    routes![get_enrollment, replace_enrollment, enrollment_info, join]
}

const INVALID_TOKEN: &str = "This install link isn't valid anymore. Ask your IT provider for a new one.";

/// One of this MSP's clients, or "not found".
async fn msp_client(msp_id: &MspId, org_id: &OrganizationId, conn: &DbConn) -> Result<MspClient, crate::error::Error> {
    match MspClient::find_by_org(org_id, conn).await {
        Some(client) if &client.msp_uuid == msp_id => Ok(client),
        _ => err_code!("Client not found", rocket::http::Status::NotFound.code),
    }
}

fn enrollment_json(client: &MspClient, joins: i64) -> serde_json::Value {
    json!({
        "token": client.enrollment_token,
        "createdAt": client.enrollment_token_created_at.as_ref().map(crate::util::format_date),
        "joins": joins,
    })
}

/// A client's enrollment token, for its install command. Any of the MSP's staff.
#[get("/keykeeper/msp/<msp_id>/clients/<org_id>/enrollment")]
async fn get_enrollment(msp_id: MspId, org_id: OrganizationId, headers: Headers, conn: DbConn) -> JsonResult {
    staff_role(&msp_id, &headers.user.uuid, &conn).await?;
    let client = msp_client(&msp_id, &org_id, &conn).await?;
    let joins = MspEnrollmentJoin::count_by_org(&org_id, &conn).await;
    Ok(Json(enrollment_json(&client, joins)))
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
    let joins = MspEnrollmentJoin::count_by_org(&org_id, &conn).await;
    Ok(Json(enrollment_json(&client, joins)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnrollmentTokenData {
    token: String,
}

/// Which organization a token joins, and what's needed to join it. The
/// public key is for account recovery enrollment.
#[post("/keykeeper/enrollment/info", data = "<data>")]
async fn enrollment_info(data: Json<EnrollmentTokenData>, _headers: Headers, conn: DbConn) -> JsonResult {
    let Some(client) = MspClient::find_by_enrollment_token(&data.token, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
    let Some(org) = Organization::find_by_uuid(&client.org_uuid, &conn).await else {
        err_code!(INVALID_TOKEN, rocket::http::Status::NotFound.code)
    };
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
        // Invited by email, joining through the install command instead.
        Some(member) => member,
        None => {
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
    super::accept_org_invite(&headers.user, member, reset_password_key, &conn).await?;

    if let Some(collection) = &client.everyone_collection {
        CollectionUser::save(&headers.user.uuid, collection, false, false, false, &conn).await?;
    }
    MspEnrollmentJoin::new(member_id, org.uuid.clone()).insert(&conn).await?;
    Audit::by("joined_with_install_command", &headers)
        .org(&org.uuid, &org.name)
        .target(&headers.user.email)
        .record(&client.msp_uuid, &conn)
        .await?;

    Ok(joined(MembershipStatus::Accepted as i32))
}

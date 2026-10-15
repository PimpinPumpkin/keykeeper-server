// Keykeeper: the MSP API, under /api/keykeeper. An MSP's staff are ordinary
// members of each client organization (MSP admins as owners, technicians as
// admins), so client work uses the upstream organization endpoints. These
// endpoints only keep that membership in step with the MSP's staff and
// clients. The server never handles a key it could decrypt with: new
// memberships wait to be confirmed by a member who holds the org key, which the
// MSP console does automatically. See DESIGN.md, "MSP console".
//
// Callers who aren't staff of an MSP get "not found", so MSPs can't be probed.

use std::collections::HashSet;

use rocket::{Route, serde::json::Json};
use serde_json::Value;

use crate::{
    CONFIG,
    api::{EmptyResult, JsonResult},
    auth::Headers,
    db::{
        DbConn,
        models::{
            Collection, Membership, MembershipId, MembershipStatus, MembershipType, Msp, MspClient, MspId,
            MspMembership, MspRole, MspStaff, OrgPolicy, OrgPolicyType, Organization, OrganizationId, User, UserId,
        },
    },
};

pub fn routes() -> Vec<Route> {
    routes![
        list_msps,
        create_msp,
        get_msp,
        create_client,
        add_staff,
        change_staff_role,
        remove_staff,
        pending_confirmations,
        get_organization_msp,
        remove_organization_msp,
    ]
}

const MAX_NAME_LENGTH: usize = 100;

fn clean_name(name: &str) -> Result<String, crate::error::Error> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME_LENGTH {
        err!(format!("Name must be 1 to {MAX_NAME_LENGTH} characters"))
    }
    Ok(name.to_owned())
}

/// The caller's role in an MSP. Anyone who isn't staff gets "not found".
async fn staff_role(msp_id: &MspId, user_id: &UserId, conn: &DbConn) -> Result<MspRole, crate::error::Error> {
    let Some(role) = MspStaff::find(msp_id, user_id, conn).await.and_then(|s| s.role()) else {
        err_code!("MSP not found", rocket::http::Status::NotFound.code)
    };
    Ok(role)
}

async fn require_admin(msp_id: &MspId, user_id: &UserId, conn: &DbConn) -> EmptyResult {
    match staff_role(msp_id, user_id, conn).await? {
        MspRole::Admin => Ok(()),
        MspRole::Technician => err_code!("Only MSP admins can do this", rocket::http::Status::Forbidden.code),
    }
}

/// Whether a membership came from an MSP. MSP staff are never part of a
/// client's account recovery: recovery lets the client's admins reset the
/// staff member's master password and take over their account, which can see
/// every other client of the MSP.
pub(super) async fn is_msp_membership(member_id: &MembershipId, conn: &DbConn) -> bool {
    MspMembership::find_by_membership(member_id, conn).await.is_some()
}

/// The memberships this MSP created, as a set.
async fn msp_membership_ids(msp_id: &MspId, conn: &DbConn) -> HashSet<MembershipId> {
    MspMembership::find_by_msp(msp_id, conn).await.into_iter().map(|m| m.membership_uuid).collect()
}

/// Gives someone the MSP's access to one client: a new membership waiting to
/// be confirmed, recorded as the MSP's. Leaves any membership they already
/// have alone, since the client granted it.
async fn add_msp_membership(
    msp_id: &MspId,
    org_id: &OrganizationId,
    user_id: &UserId,
    role: MspRole,
    invited_by: &str,
    conn: &DbConn,
) -> EmptyResult {
    if Membership::find_by_user_and_org(user_id, org_id, conn).await.is_some() {
        return Ok(());
    }
    let mut member = Membership::new(user_id.clone(), org_id.clone(), Some(invited_by.to_owned()));
    member.atype = role.membership_type() as i32;
    member.access_all = true;
    member.status = MembershipStatus::Accepted as i32;
    member.save(conn).await?;
    MspMembership::new(member.uuid, msp_id.clone()).insert(conn).await
}

/// Confirmed owners of an organization, other than one membership.
async fn other_confirmed_owners(org_id: &OrganizationId, except: &MembershipId, conn: &DbConn) -> usize {
    Membership::find_by_org(org_id, conn)
        .await
        .iter()
        .filter(|m| {
            &m.uuid != except
                && m.atype == MembershipType::Owner as i32
                && m.status == MembershipStatus::Confirmed as i32
        })
        .count()
}

/// Someone's MSP memberships in this MSP's clients.
async fn staff_memberships(msp_id: &MspId, user_id: &UserId, conn: &DbConn) -> Vec<Membership> {
    let ours = msp_membership_ids(msp_id, conn).await;
    let mut result = Vec::new();
    for client in MspClient::find_by_msp(msp_id, conn).await {
        if let Some(member) = Membership::find_by_user_and_org(user_id, &client.org_uuid, conn).await
            && ours.contains(&member.uuid)
        {
            result.push(member);
        }
    }
    result
}

/// Refuses a change that would leave a client with no confirmed owner.
async fn ensure_owners_remain(memberships: &[Membership], conn: &DbConn) -> EmptyResult {
    for member in memberships {
        let is_confirmed_owner =
            member.atype == MembershipType::Owner as i32 && member.status == MembershipStatus::Confirmed as i32;
        if is_confirmed_owner && other_confirmed_owners(&member.org_uuid, &member.uuid, conn).await == 0 {
            let name = Organization::find_by_uuid(&member.org_uuid, conn).await.map(|o| o.name).unwrap_or_default();
            err!(format!("{name} would have no owner. Make someone else an owner there first."))
        }
    }
    Ok(())
}

#[get("/keykeeper/msp")]
async fn list_msps(headers: Headers, conn: DbConn) -> Json<Value> {
    let msps: Vec<Value> = Msp::find_by_staff_user(&headers.user.uuid, &conn)
        .await
        .into_iter()
        .filter_map(|(msp, staff)| {
            staff.role().map(|role| {
                json!({
                    "id": msp.uuid,
                    "name": msp.name,
                    "role": role.as_str(),
                })
            })
        })
        .collect();
    Json(json!({ "data": msps }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMspData {
    name: String,
}

#[post("/keykeeper/msp", data = "<data>")]
async fn create_msp(headers: Headers, data: Json<CreateMspData>, conn: DbConn) -> JsonResult {
    if !CONFIG.msp_signups_allowed() {
        err_code!("This server doesn't allow creating MSPs", rocket::http::Status::Forbidden.code)
    }
    let msp = Msp::new(clean_name(&data.name)?);
    msp.insert(&conn).await?;
    MspStaff::new(msp.uuid.clone(), headers.user.uuid.clone(), MspRole::Admin).insert(&conn).await?;
    Ok(Json(json!({ "id": msp.uuid, "name": msp.name, "role": MspRole::Admin.as_str() })))
}

#[get("/keykeeper/msp/<msp_id>")]
async fn get_msp(msp_id: MspId, headers: Headers, conn: DbConn) -> JsonResult {
    let role = staff_role(&msp_id, &headers.user.uuid, &conn).await?;
    let Some(msp) = Msp::find_by_uuid(&msp_id, &conn).await else {
        err_code!("MSP not found", rocket::http::Status::NotFound.code)
    };

    let mut staff = Vec::new();
    for member in MspStaff::find_by_msp(&msp_id, &conn).await {
        if let (Some(user), Some(member_role)) = (User::find_by_uuid(&member.user_uuid, &conn).await, member.role()) {
            staff.push(json!({
                "userId": user.uuid,
                "email": user.email,
                "name": user.name,
                "role": member_role.as_str(),
            }));
        }
    }

    // Member counts are the client's own people; the MSP's staff are left out.
    let ours = msp_membership_ids(&msp_id, &conn).await;
    let mut clients = Vec::new();
    for client in MspClient::find_by_msp(&msp_id, &conn).await {
        let Some(org) = Organization::find_by_uuid(&client.org_uuid, &conn).await else {
            continue;
        };
        let (mut confirmed, mut accepted, mut invited, mut revoked) = (0, 0, 0, 0);
        let mut you = Value::Null;
        for member in Membership::find_by_org(&org.uuid, &conn).await {
            if member.user_uuid == headers.user.uuid {
                you = json!({ "memberId": member.uuid, "status": member.status, "type": member.atype });
            }
            if ours.contains(&member.uuid) {
                continue;
            }
            match member.status {
                s if s == MembershipStatus::Confirmed as i32 => confirmed += 1,
                s if s == MembershipStatus::Accepted as i32 => accepted += 1,
                s if s == MembershipStatus::Invited as i32 => invited += 1,
                _ => revoked += 1,
            }
        }
        clients.push(json!({
            "id": org.uuid,
            "name": org.name,
            "preset": client.preset,
            "createdAt": crate::util::format_date(&client.created_at),
            "members": { "confirmed": confirmed, "accepted": accepted, "invited": invited, "revoked": revoked },
            "you": you,
        }));
    }

    Ok(Json(json!({
        "id": msp.uuid,
        "name": msp.name,
        "role": role.as_str(),
        "staff": staff,
        "clients": clients,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientKeys {
    encrypted_private_key: String,
    public_key: String,
}

/// The Business preset: the defaults a business client starts with, so its
/// admins never have to build policies to get them. Every one is an upstream
/// policy the client's owners can change later in its admin console. See
/// DESIGN.md, "Defaults (Business preset)".
const BUSINESS_PRESET: &str = "business";

fn business_policies() -> Vec<(OrgPolicyType, Value)> {
    vec![
        // Account recovery, with automatic enrollment: a member who forgets
        // their master password can be let back in by an owner or admin.
        (OrgPolicyType::ResetPassword, json!({ "autoEnrollEnabled": true })),
        // Single organization: members can't also belong to another
        // organization with the same account. Recovery opens a member's whole
        // account, so without this a client's admins could recover their way
        // into a member's family organization. Personal items go in a separate
        // personal account. Upstream exempts owners and admins, so MSP staff
        // still reach every client.
        (OrgPolicyType::SingleOrg, Value::Null),
        // A strong master password: at least 12 characters and a strength
        // score of 3 out of 4. Existing passwords aren't forced to change.
        (
            OrgPolicyType::MasterPassword,
            json!({
                "minComplexity": 3,
                "minLength": 12,
                "requireUpper": false,
                "requireLower": false,
                "requireNumbers": false,
                "requireSpecial": false,
                "enforceOnLogin": false,
            }),
        ),
    ]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateClientData {
    name: String,
    billing_email: Option<String>,
    /// The default collection's name, encrypted with the new org key. With the
    /// Business preset, this is the "Everyone" collection.
    collection_name: String,
    /// With the Business preset: the "Admins" collection's name, encrypted with
    /// the new org key.
    admins_collection_name: Option<String>,
    /// "business" for the Business preset, or none.
    preset: Option<String>,
    /// The new org key, encrypted for the caller.
    key: String,
    keys: ClientKeys,
}

/// Creates a client organization, owned by the caller, with every other staff
/// member added and waiting to be confirmed.
#[post("/keykeeper/msp/<msp_id>/clients", data = "<data>")]
async fn create_client(msp_id: MspId, headers: Headers, data: Json<CreateClientData>, conn: DbConn) -> JsonResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let data = data.into_inner();
    let name = clean_name(&data.name)?;
    let business = match data.preset.as_deref() {
        None | Some("none") => false,
        Some(BUSINESS_PRESET) => true,
        Some(_) => err!("Preset must be business or none"),
    };
    let admins_collection = match (business, data.admins_collection_name) {
        (true, Some(admins)) => Some(admins),
        (true, None) => err!("The Business preset needs the Admins collection's name"),
        (false, _) => None,
    };
    let billing_email = data.billing_email.filter(|e| !e.trim().is_empty()).unwrap_or(headers.user.email.clone());

    let org =
        Organization::new(name, &billing_email, Some(data.keys.encrypted_private_key), Some(data.keys.public_key));
    let mut owner = Membership::new(headers.user.uuid.clone(), org.uuid.clone(), None);
    owner.akey = data.key;
    owner.access_all = true;
    owner.atype = MembershipType::Owner as i32;
    owner.status = MembershipStatus::Confirmed as i32;
    let collection = Collection::new(org.uuid.clone(), data.collection_name, None);

    org.save(&conn).await?;
    owner.save(&conn).await?;
    collection.save(&conn).await?;
    if let Some(admins) = admins_collection {
        Collection::new(org.uuid.clone(), admins, None).save(&conn).await?;
    }
    if business {
        for (policy_type, policy_data) in business_policies() {
            OrgPolicy::new(org.uuid.clone(), policy_type, true, policy_data.to_string()).save(&conn).await?;
        }
    }
    MspClient::new(org.uuid.clone(), msp_id.clone(), business.then(|| BUSINESS_PRESET.to_owned()))
        .insert(&conn)
        .await?;
    MspMembership::new(owner.uuid.clone(), msp_id.clone()).insert(&conn).await?;

    for staff in MspStaff::find_by_msp(&msp_id, &conn).await {
        if staff.user_uuid == headers.user.uuid {
            continue;
        }
        if let Some(role) = staff.role() {
            add_msp_membership(&msp_id, &org.uuid, &staff.user_uuid, role, &headers.user.email, &conn).await?;
        }
    }

    Ok(Json(org.to_json()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddStaffData {
    email: String,
    role: String,
}

/// Adds someone with an account to the MSP, and to every client.
#[post("/keykeeper/msp/<msp_id>/staff", data = "<data>")]
async fn add_staff(msp_id: MspId, headers: Headers, data: Json<AddStaffData>, conn: DbConn) -> JsonResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let Some(role) = MspRole::from_str(&data.role) else {
        err!("Role must be admin or technician")
    };
    let Some(user) = User::find_by_mail(data.email.trim(), &conn).await else {
        err!("No account uses that email. Ask them to create one first.")
    };
    if MspStaff::find(&msp_id, &user.uuid, &conn).await.is_some() {
        err!("They're already on your staff")
    }
    MspStaff::new(msp_id.clone(), user.uuid.clone(), role).insert(&conn).await?;
    for client in MspClient::find_by_msp(&msp_id, &conn).await {
        add_msp_membership(&msp_id, &client.org_uuid, &user.uuid, role, &headers.user.email, &conn).await?;
    }
    Ok(Json(json!({ "userId": user.uuid, "email": user.email, "name": user.name, "role": role.as_str() })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangeStaffRoleData {
    role: String,
}

/// Changes a staff member's role, and their role in every client with it.
#[put("/keykeeper/msp/<msp_id>/staff/<user_id>", data = "<data>")]
async fn change_staff_role(
    msp_id: MspId,
    user_id: UserId,
    headers: Headers,
    data: Json<ChangeStaffRoleData>,
    conn: DbConn,
) -> EmptyResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let Some(role) = MspRole::from_str(&data.role) else {
        err!("Role must be admin or technician")
    };
    let Some(mut staff) = MspStaff::find(&msp_id, &user_id, &conn).await else {
        err_code!("Staff member not found", rocket::http::Status::NotFound.code)
    };
    if staff.role() == Some(role) {
        return Ok(());
    }
    if role == MspRole::Technician {
        ensure_another_admin(&msp_id, &user_id, &conn).await?;
    }
    let mut memberships = staff_memberships(&msp_id, &user_id, &conn).await;
    if role == MspRole::Technician {
        ensure_owners_remain(&memberships, &conn).await?;
    }
    staff.set_role(role, &conn).await?;
    for member in &mut memberships {
        member.atype = role.membership_type() as i32;
        member.save(&conn).await?;
    }
    Ok(())
}

async fn ensure_another_admin(msp_id: &MspId, user_id: &UserId, conn: &DbConn) -> EmptyResult {
    let other_admins = MspStaff::find_by_msp(msp_id, conn)
        .await
        .iter()
        .filter(|s| &s.user_uuid != user_id && s.role() == Some(MspRole::Admin))
        .count();
    if other_admins == 0 {
        err!("An MSP needs at least one admin")
    }
    Ok(())
}

/// Removes someone from the MSP and from every client, at once.
#[delete("/keykeeper/msp/<msp_id>/staff/<user_id>")]
async fn remove_staff(msp_id: MspId, user_id: UserId, headers: Headers, conn: DbConn) -> EmptyResult {
    require_admin(&msp_id, &headers.user.uuid, &conn).await?;
    let Some(staff) = MspStaff::find(&msp_id, &user_id, &conn).await else {
        err_code!("Staff member not found", rocket::http::Status::NotFound.code)
    };
    if staff.role() == Some(MspRole::Admin) {
        ensure_another_admin(&msp_id, &user_id, &conn).await?;
    }
    let memberships = staff_memberships(&msp_id, &user_id, &conn).await;
    ensure_owners_remain(&memberships, &conn).await?;
    for member in memberships {
        member.delete(&conn).await?;
    }
    staff.delete(&conn).await
}

/// Memberships waiting for the org key in clients where the caller can give
/// it: the caller is a confirmed owner or admin there. The console confirms
/// them with the upstream confirm endpoint.
#[get("/keykeeper/msp/<msp_id>/pending")]
async fn pending_confirmations(msp_id: MspId, headers: Headers, conn: DbConn) -> JsonResult {
    staff_role(&msp_id, &headers.user.uuid, &conn).await?;
    let ours = msp_membership_ids(&msp_id, &conn).await;
    let mut pending = Vec::new();
    for client in MspClient::find_by_msp(&msp_id, &conn).await {
        let can_confirm =
            Membership::find_by_user_and_org(&headers.user.uuid, &client.org_uuid, &conn).await.is_some_and(|m| {
                m.status == MembershipStatus::Confirmed as i32
                    && (m.atype == MembershipType::Owner as i32 || m.atype == MembershipType::Admin as i32)
            });
        if !can_confirm {
            continue;
        }
        for member in Membership::find_by_org(&client.org_uuid, &conn).await {
            if member.status != MembershipStatus::Accepted as i32 {
                continue;
            }
            let Some(user) = User::find_by_uuid(&member.user_uuid, &conn).await else {
                continue;
            };
            let Some(public_key) = user.public_key.clone() else {
                continue;
            };
            pending.push(json!({
                "organizationId": client.org_uuid,
                "memberId": member.uuid,
                "userId": user.uuid,
                "email": user.email,
                "name": user.name,
                "publicKey": public_key,
                "fromMsp": ours.contains(&member.uuid),
            }));
        }
    }
    Ok(Json(json!({ "data": pending })))
}

/// For a client's own owners and admins: which MSP manages the organization,
/// and its staff there.
#[get("/keykeeper/organizations/<org_id>/msp")]
async fn get_organization_msp(org_id: OrganizationId, headers: Headers, conn: DbConn) -> JsonResult {
    let Some(caller) = Membership::find_confirmed_by_user_and_org(&headers.user.uuid, &org_id, &conn).await else {
        err_code!("Organization not found", rocket::http::Status::NotFound.code)
    };
    if caller.atype != MembershipType::Owner as i32 && caller.atype != MembershipType::Admin as i32 {
        err_code!("Only owners and admins can see this", rocket::http::Status::Forbidden.code)
    }
    let Some(client) = MspClient::find_by_org(&org_id, &conn).await else {
        return Ok(Json(json!({ "msp": null, "staff": [] })));
    };
    let Some(msp) = Msp::find_by_uuid(&client.msp_uuid, &conn).await else {
        return Ok(Json(json!({ "msp": null, "staff": [] })));
    };
    let ours = msp_membership_ids(&msp.uuid, &conn).await;
    let mut staff = Vec::new();
    for member in Membership::find_by_org(&org_id, &conn).await {
        if !ours.contains(&member.uuid) {
            continue;
        }
        if let Some(user) = User::find_by_uuid(&member.user_uuid, &conn).await {
            staff.push(json!({
                "memberId": member.uuid,
                "email": user.email,
                "name": user.name,
                "type": member.atype,
                "status": member.status,
            }));
        }
    }
    Ok(Json(json!({ "msp": { "id": msp.uuid, "name": msp.name }, "staff": staff })))
}

/// For a client's own owners: removes the MSP and all of its staff from the
/// organization in one step. The MSP's own staff can't do this.
#[delete("/keykeeper/organizations/<org_id>/msp")]
async fn remove_organization_msp(org_id: OrganizationId, headers: Headers, conn: DbConn) -> EmptyResult {
    let Some(caller) = Membership::find_confirmed_by_user_and_org(&headers.user.uuid, &org_id, &conn).await else {
        err_code!("Organization not found", rocket::http::Status::NotFound.code)
    };
    let Some(client) = MspClient::find_by_org(&org_id, &conn).await else {
        err_code!("This organization has no MSP", rocket::http::Status::NotFound.code)
    };
    let caller_is_msp = MspMembership::find_by_membership(&caller.uuid, &conn).await.is_some();
    if caller.atype != MembershipType::Owner as i32 || caller_is_msp {
        err_code!("Only the organization's own owners can remove its MSP", rocket::http::Status::Forbidden.code)
    }
    let ours = msp_membership_ids(&client.msp_uuid, &conn).await;
    for member in Membership::find_by_org(&org_id, &conn).await {
        if ours.contains(&member.uuid) {
            member.delete(&conn).await?;
        }
    }
    client.delete(&conn).await
}

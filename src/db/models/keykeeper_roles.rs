// Keykeeper: custom roles with real permissions. Upstream's member dialog
// offers a "Custom" role with permissions; Vaultwarden ignores them and treats
// Custom as the old Manager role. Here a custom member is a regular member
// (upstream's User type) plus the permissions in this table, which the admin
// guard honors only for the endpoints listed in ROUTES below.
//
// A custom member acts with an admin's rules but only on regular members: never
// on themselves, admins, owners, or other custom members, and they can't grant
// roles, permissions, or access to all items. See DESIGN.md, "Roles and
// permissions".

use std::collections::HashMap;

use diesel::prelude::*;
use rocket::{Request, http::Method};
use serde_json::Value;

use crate::{
    api::EmptyResult,
    db::{DbConn, keykeeper_schema::membership_permissions},
    error::MapResult,
};

use super::{Membership, MembershipId, MembershipType, UserId};

/// What a custom role can be allowed to do, by upstream's permission names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// Invite, confirm, remove, revoke, restore, and edit regular members.
    ManageUsers,
    /// Recover regular members' accounts.
    ManageResetPassword,
    ManagePolicies,
    AccessEventLogs,
    /// Export the organization's vault.
    AccessImportExport,
}

impl Permission {
    pub const ALL: [Permission; 5] =
        [ManageUsers, ManageResetPassword, ManagePolicies, AccessEventLogs, AccessImportExport];

    /// The name upstream's clients use.
    pub fn key(self) -> &'static str {
        match self {
            ManageUsers => "manageUsers",
            ManageResetPassword => "manageResetPassword",
            ManagePolicies => "managePolicies",
            AccessEventLogs => "accessEventLogs",
            AccessImportExport => "accessImportExport",
        }
    }
}

use Permission::{AccessEventLogs, AccessImportExport, ManagePolicies, ManageResetPassword, ManageUsers};

/// The endpoints a custom role can reach, and the permissions that each
/// accepts (any one of them). Every other admin endpoint needs an owner or
/// admin. Paths are route templates, without the "/api" mount or the query.
const ROUTES: &[(Method, &str, &[Permission])] = &[
    // Members
    (Method::Get, "/organizations/<org_id>/users", &[ManageUsers, ManageResetPassword]),
    (Method::Get, "/organizations/<org_id>/users/<member_id>", &[ManageUsers, ManageResetPassword]),
    (Method::Post, "/organizations/<org_id>/users/invite", &[ManageUsers]),
    (Method::Post, "/organizations/<org_id>/users/reinvite", &[ManageUsers]),
    (Method::Post, "/organizations/<org_id>/users/<member_id>/reinvite", &[ManageUsers]),
    (Method::Post, "/organizations/<org_id>/users/public-keys", &[ManageUsers]),
    (Method::Post, "/organizations/<org_id>/users/confirm", &[ManageUsers]),
    (Method::Post, "/organizations/<org_id>/users/<member_id>/confirm", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/<member_id>", &[ManageUsers]),
    (Method::Delete, "/organizations/<org_id>/users", &[ManageUsers]),
    (Method::Delete, "/organizations/<org_id>/users/<member_id>", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/revoke", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/<member_id>/revoke", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/restore", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/<member_id>/restore", &[ManageUsers]),
    (Method::Put, "/organizations/<org_id>/users/<member_id>/restore/vnext", &[ManageUsers]),
    // Account recovery
    (Method::Get, "/organizations/<org_id>/users/<member_id>/reset-password-details", &[ManageResetPassword]),
    (Method::Put, "/organizations/<org_id>/users/<member_id>/recover-account", &[ManageResetPassword]),
    // Policies
    (Method::Get, "/organizations/<org_id>/policies", &[ManagePolicies]),
    (Method::Get, "/organizations/<org_id>/policies/<pol_type>", &[ManagePolicies]),
    (Method::Put, "/organizations/<org_id>/policies/<pol_type>", &[ManagePolicies]),
    // Event logs
    (Method::Get, "/organizations/<org_id>/events", &[AccessEventLogs]),
    (Method::Get, "/organizations/<org_id>/users/<member_id>/events", &[AccessEventLogs]),
    // Export
    (Method::Get, "/organizations/<org_id>/export", &[AccessImportExport]),
];

/// The permissions that admit a custom role to this request's route, if any.
pub fn route_permissions(method: Method, path: &str) -> &'static [Permission] {
    ROUTES.iter().find(|(m, p, _)| *m == method && *p == path).map_or(&[], |(_, _, perms)| *perms)
}

/// A custom role's permissions.
#[expect(clippy::struct_excessive_bools, reason = "One flag per upstream permission, as clients send them")]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomPermissions {
    #[serde(default)]
    pub manage_users: bool,
    #[serde(default)]
    pub manage_reset_password: bool,
    #[serde(default)]
    pub manage_policies: bool,
    #[serde(default)]
    pub access_event_logs: bool,
    #[serde(default)]
    pub access_import_export: bool,
}

impl CustomPermissions {
    pub fn has(&self, permission: Permission) -> bool {
        match permission {
            ManageUsers => self.manage_users,
            ManageResetPassword => self.manage_reset_password,
            ManagePolicies => self.manage_policies,
            AccessEventLogs => self.access_event_logs,
            AccessImportExport => self.access_import_export,
        }
    }

    fn any(&self) -> bool {
        Permission::ALL.iter().any(|p| self.has(*p))
    }

    /// The custom role an invite or edit asks for: upstream's Custom type ("4")
    /// with at least one of our permissions. Otherwise None, and upstream's own
    /// handling of Custom applies.
    pub fn from_request(raw_type: &str, permissions: &HashMap<String, Value>) -> Option<Self> {
        if raw_type != "4" && raw_type != "Custom" {
            return None;
        }
        let granted = |p: Permission| permissions.get(p.key()) == Some(&Value::Bool(true));
        let role = Self {
            manage_users: granted(ManageUsers),
            manage_reset_password: granted(ManageResetPassword),
            manage_policies: granted(ManagePolicies),
            access_event_logs: granted(AccessEventLogs),
            access_import_export: granted(AccessImportExport),
        };
        role.any().then_some(role)
    }

    /// Upstream's full permissions object, as clients expect it.
    pub fn to_client_json(&self, all_collections: bool) -> Value {
        json!({
            "accessEventLogs": self.access_event_logs,
            "accessImportExport": self.access_import_export,
            "accessReports": false,
            "createNewCollections": all_collections,
            "editAnyCollection": all_collections,
            "deleteAnyCollection": all_collections,
            "manageGroups": false,
            "managePolicies": self.manage_policies,
            "manageSso": false,
            "manageUsers": self.manage_users,
            "manageResetPassword": self.manage_reset_password,
            "manageScim": false,
        })
    }
}

#[derive(Identifiable, Queryable, Insertable)]
#[diesel(table_name = membership_permissions)]
#[diesel(primary_key(membership_uuid))]
struct MembershipPermissions {
    membership_uuid: MembershipId,
    permissions: String,
}

/// A membership's custom role, if it has one.
pub async fn find(member_id: &MembershipId, conn: &DbConn) -> Option<CustomPermissions> {
    let row = conn
        .run(move |conn| {
            membership_permissions::table
                .filter(membership_permissions::membership_uuid.eq(member_id))
                .first::<MembershipPermissions>(conn)
                .ok()
        })
        .await?;
    serde_json::from_str(&row.permissions).ok()
}

/// Gives a membership a custom role, or takes it away with None.
pub async fn set(member_id: &MembershipId, role: Option<&CustomPermissions>, conn: &DbConn) -> EmptyResult {
    let id = member_id.clone();
    let deleted: EmptyResult = conn
        .run(move |conn| {
            diesel::delete(membership_permissions::table.filter(membership_permissions::membership_uuid.eq(id)))
                .execute(conn)
                .map_res("Error removing a custom role")
        })
        .await;
    deleted?;
    let Some(role) = role else {
        return Ok(());
    };
    let row = MembershipPermissions {
        membership_uuid: member_id.clone(),
        permissions: serde_json::to_string(role).unwrap_or_default(),
    };
    conn.run(move |conn| {
        diesel::insert_into(membership_permissions::table)
            .values(&row)
            .execute(conn)
            .map_res("Error saving a custom role")
    })
    .await
}

/// For the admin guard: the custom role that admits this member to this
/// request, if their permissions cover its route. Owners and admins don't use
/// this; they're admitted as before.
pub async fn admitted(request: &Request<'_>, member: &Membership) -> Option<CustomPermissions> {
    let route = request.route()?;
    let needed = route_permissions(route.method, route.uri.unmounted_origin.path().as_str());
    if needed.is_empty() || member.atype != MembershipType::User as i32 {
        return None;
    }
    let rocket::outcome::Outcome::Success(conn) = request.guard::<DbConn>().await else {
        return None;
    };
    let role = find(&member.uuid, &conn).await?;
    needed.iter().any(|p| role.has(*p)).then_some(role)
}

/// Refuses a custom member's action on anyone but a regular member: not
/// themselves, admins, owners, or other custom members.
pub async fn ensure_regular_target(
    acting_as: Option<&CustomPermissions>,
    actor: &UserId,
    target: &Membership,
    conn: &DbConn,
) -> EmptyResult {
    if acting_as.is_none() {
        return Ok(());
    }
    if &target.user_uuid == actor {
        err!("You can't change your own membership")
    }
    if target.atype != MembershipType::User as i32 || find(&target.uuid, conn).await.is_some() {
        err!("Your role can only manage regular members")
    }
    Ok(())
}

/// Refuses a custom member's attempt to grant more than a regular member:
/// another role, a custom role, or access to all items.
pub fn ensure_regular_grant(
    acting_as: Option<&CustomPermissions>,
    new_type: i32,
    custom_role: Option<&CustomPermissions>,
    access_all: bool,
) -> EmptyResult {
    if acting_as.is_some() && (new_type != MembershipType::User as i32 || custom_role.is_some() || access_all) {
        err!("Your role can only add regular members, without access to all items")
    }
    Ok(())
}

/// What clients see for a membership: Custom (4) with the role's permissions,
/// or whatever upstream decided.
pub async fn client_type_and_permissions(
    member: &Membership,
    upstream_type: i32,
    upstream_permissions: Value,
    conn: &DbConn,
) -> (i32, Value) {
    match find(&member.uuid, conn).await {
        Some(role) => (4, role.to_client_json(member.access_all)),
        None => (upstream_type, upstream_permissions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_has_a_permission_and_appears_once() {
        for (i, (method, path, perms)) in ROUTES.iter().enumerate() {
            assert!(!perms.is_empty(), "{method} {path} has no permission");
            assert!(!ROUTES[i + 1..].iter().any(|(m, p, _)| m == method && p == path), "{method} {path} appears twice");
        }
    }

    #[test]
    fn unknown_routes_admit_no_custom_role() {
        assert!(route_permissions(Method::Delete, "/organizations/<org_id>").is_empty());
        assert!(route_permissions(Method::Post, "/organizations/<org_id>/api-key").is_empty());
        assert!(route_permissions(Method::Put, "/organizations/<org_id>").is_empty());
        assert_eq!(route_permissions(Method::Put, "/organizations/<org_id>/policies/<pol_type>"), &[ManagePolicies]);
    }

    #[test]
    fn only_custom_type_with_our_permissions_is_a_custom_role() {
        let perms = |pairs: &[(&str, bool)]| -> HashMap<String, Value> {
            pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::Bool(*v))).collect()
        };
        assert_eq!(CustomPermissions::from_request("2", &perms(&[("manageUsers", true)])), None);
        assert_eq!(CustomPermissions::from_request("4", &perms(&[("editAnyCollection", true)])), None);
        assert_eq!(CustomPermissions::from_request("4", &perms(&[("manageUsers", false)])), None);
        let role = CustomPermissions::from_request("4", &perms(&[("manageUsers", true), ("managePolicies", true)]));
        assert_eq!(
            role,
            Some(CustomPermissions {
                manage_users: true,
                manage_policies: true,
                ..Default::default()
            })
        );
    }
}

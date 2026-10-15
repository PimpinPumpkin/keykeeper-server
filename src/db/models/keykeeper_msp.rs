// Keykeeper: MSPs (managed service providers). An MSP has staff and client
// organizations. Staff are ordinary members of each client organization, and
// the memberships an MSP created are recorded in msp_memberships so they can be
// listed, re-roled, and removed together. No keys live here: staff get each
// organization's key the normal way, when a member who holds it confirms them.
// See DESIGN.md, "MSP console", in the keykeeper repo.

use chrono::{NaiveDateTime, Utc};
use derive_more::{AsRef, Deref, Display, From};
use diesel::prelude::*;

use crate::{
    api::EmptyResult,
    db::{
        DbConn,
        keykeeper_schema::{msp_clients, msp_memberships, msp_staff, msps},
    },
    error::MapResult,
};
use macros::UuidFromParam;

use super::{MembershipId, MembershipType, OrganizationId, UserId};

// Our migrations exist for SQLite and PostgreSQL only.
#[cfg(feature = "mysql")]
compile_error!("Keykeeper supports SQLite and PostgreSQL only. See DESIGN.md, \"MSP console\".");

#[derive(
    Clone,
    Debug,
    AsRef,
    Deref,
    DieselNewType,
    Display,
    From,
    FromForm,
    Hash,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    UuidFromParam,
)]
#[deref(forward)]
#[from(forward)]
pub struct MspId(String);

/// What someone may do in an MSP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MspRole {
    /// Manages the MSP, its staff, and its clients. An owner in every client.
    Admin = 0,
    /// Works in client organizations. An admin in every client.
    Technician = 1,
}

impl MspRole {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Admin),
            1 => Some(Self::Technician),
            _ => None,
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "technician" => Some(Self::Technician),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Technician => "technician",
        }
    }

    /// The role this gives someone in each client organization.
    pub fn membership_type(self) -> MembershipType {
        match self {
            Self::Admin => MembershipType::Owner,
            Self::Technician => MembershipType::Admin,
        }
    }
}

#[derive(Identifiable, Queryable, Insertable)]
#[diesel(table_name = msps)]
#[diesel(primary_key(uuid))]
pub struct Msp {
    pub uuid: MspId,
    pub name: String,
    pub created_at: NaiveDateTime,
}

#[derive(Identifiable, Queryable, Insertable)]
#[diesel(table_name = msp_staff)]
#[diesel(primary_key(msp_uuid, user_uuid))]
pub struct MspStaff {
    pub msp_uuid: MspId,
    pub user_uuid: UserId,
    pub role: i32,
    pub created_at: NaiveDateTime,
}

#[derive(Identifiable, Queryable, Insertable)]
#[diesel(table_name = msp_clients)]
#[diesel(primary_key(org_uuid))]
pub struct MspClient {
    pub org_uuid: OrganizationId,
    pub msp_uuid: MspId,
    pub created_at: NaiveDateTime,
    /// The defaults the client started with, such as "business".
    pub preset: Option<String>,
}

#[derive(Identifiable, Queryable, Insertable)]
#[diesel(table_name = msp_memberships)]
#[diesel(primary_key(membership_uuid))]
pub struct MspMembership {
    pub membership_uuid: MembershipId,
    pub msp_uuid: MspId,
}

impl Msp {
    pub fn new(name: String) -> Self {
        Self {
            uuid: MspId(crate::util::get_uuid()),
            name,
            created_at: Utc::now().naive_utc(),
        }
    }

    pub async fn insert(&self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| diesel::insert_into(msps::table).values(self).execute(conn).map_res("Error saving MSP"))
            .await
    }

    pub async fn find_by_uuid(uuid: &MspId, conn: &DbConn) -> Option<Self> {
        conn.run(move |conn| msps::table.filter(msps::uuid.eq(uuid)).first::<Self>(conn).ok()).await
    }

    /// The MSPs someone is staff of, with their role in each.
    pub async fn find_by_staff_user(user_uuid: &UserId, conn: &DbConn) -> Vec<(Self, MspStaff)> {
        let mut result = Vec::new();
        for staff in MspStaff::find_by_user(user_uuid, conn).await {
            if let Some(msp) = Self::find_by_uuid(&staff.msp_uuid, conn).await {
                result.push((msp, staff));
            }
        }
        result
    }
}

impl MspStaff {
    pub fn new(msp_uuid: MspId, user_uuid: UserId, role: MspRole) -> Self {
        Self {
            msp_uuid,
            user_uuid,
            role: role as i32,
            created_at: Utc::now().naive_utc(),
        }
    }

    pub fn role(&self) -> Option<MspRole> {
        MspRole::from_i32(self.role)
    }

    pub async fn insert(&self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| {
            diesel::insert_into(msp_staff::table).values(self).execute(conn).map_res("Error adding MSP staff")
        })
        .await
    }

    pub async fn set_role(&mut self, role: MspRole, conn: &DbConn) -> EmptyResult {
        self.role = role as i32;
        let (msp, user, role) = (&self.msp_uuid, &self.user_uuid, self.role);
        conn.run(move |conn| {
            diesel::update(msp_staff::table.filter(msp_staff::msp_uuid.eq(msp)).filter(msp_staff::user_uuid.eq(user)))
                .set(msp_staff::role.eq(role))
                .execute(conn)
                .map_res("Error changing MSP staff role")
        })
        .await
    }

    pub async fn delete(self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| {
            diesel::delete(
                msp_staff::table
                    .filter(msp_staff::msp_uuid.eq(self.msp_uuid))
                    .filter(msp_staff::user_uuid.eq(self.user_uuid)),
            )
            .execute(conn)
            .map_res("Error removing MSP staff")
        })
        .await
    }

    pub async fn find(msp_uuid: &MspId, user_uuid: &UserId, conn: &DbConn) -> Option<Self> {
        conn.run(move |conn| {
            msp_staff::table
                .filter(msp_staff::msp_uuid.eq(msp_uuid))
                .filter(msp_staff::user_uuid.eq(user_uuid))
                .first::<Self>(conn)
                .ok()
        })
        .await
    }

    pub async fn find_by_msp(msp_uuid: &MspId, conn: &DbConn) -> Vec<Self> {
        conn.run(move |conn| {
            msp_staff::table
                .filter(msp_staff::msp_uuid.eq(msp_uuid))
                .order(msp_staff::created_at.asc())
                .load::<Self>(conn)
                .unwrap_or_default()
        })
        .await
    }

    pub async fn find_by_user(user_uuid: &UserId, conn: &DbConn) -> Vec<Self> {
        conn.run(move |conn| {
            msp_staff::table.filter(msp_staff::user_uuid.eq(user_uuid)).load::<Self>(conn).unwrap_or_default()
        })
        .await
    }
}

impl MspClient {
    pub fn new(org_uuid: OrganizationId, msp_uuid: MspId, preset: Option<String>) -> Self {
        Self {
            org_uuid,
            msp_uuid,
            created_at: Utc::now().naive_utc(),
            preset,
        }
    }

    pub async fn insert(&self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| {
            diesel::insert_into(msp_clients::table).values(self).execute(conn).map_res("Error adding MSP client")
        })
        .await
    }

    pub async fn delete(self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| {
            diesel::delete(msp_clients::table.filter(msp_clients::org_uuid.eq(self.org_uuid)))
                .execute(conn)
                .map_res("Error removing MSP client")
        })
        .await
    }

    pub async fn find_by_org(org_uuid: &OrganizationId, conn: &DbConn) -> Option<Self> {
        conn.run(move |conn| msp_clients::table.filter(msp_clients::org_uuid.eq(org_uuid)).first::<Self>(conn).ok())
            .await
    }

    pub async fn find_by_msp(msp_uuid: &MspId, conn: &DbConn) -> Vec<Self> {
        conn.run(move |conn| {
            msp_clients::table
                .filter(msp_clients::msp_uuid.eq(msp_uuid))
                .order(msp_clients::created_at.asc())
                .load::<Self>(conn)
                .unwrap_or_default()
        })
        .await
    }
}

impl MspMembership {
    pub fn new(membership_uuid: MembershipId, msp_uuid: MspId) -> Self {
        Self {
            membership_uuid,
            msp_uuid,
        }
    }

    pub async fn insert(&self, conn: &DbConn) -> EmptyResult {
        conn.run(move |conn| {
            diesel::insert_into(msp_memberships::table)
                .values(self)
                .execute(conn)
                .map_res("Error recording MSP membership")
        })
        .await
    }

    pub async fn find_by_membership(membership_uuid: &MembershipId, conn: &DbConn) -> Option<Self> {
        conn.run(move |conn| {
            msp_memberships::table.filter(msp_memberships::membership_uuid.eq(membership_uuid)).first::<Self>(conn).ok()
        })
        .await
    }

    pub async fn find_by_msp(msp_uuid: &MspId, conn: &DbConn) -> Vec<Self> {
        conn.run(move |conn| {
            msp_memberships::table.filter(msp_memberships::msp_uuid.eq(msp_uuid)).load::<Self>(conn).unwrap_or_default()
        })
        .await
    }
}

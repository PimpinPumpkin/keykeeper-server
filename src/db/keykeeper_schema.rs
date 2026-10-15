// Keykeeper's own tables, kept apart from upstream's schema.rs so upstream
// merges stay clean. See the keykeeper_msp migrations.

table! {
    msps (uuid) {
        uuid -> Text,
        name -> Text,
        created_at -> Timestamp,
    }
}

table! {
    msp_staff (msp_uuid, user_uuid) {
        msp_uuid -> Text,
        user_uuid -> Text,
        role -> Integer,
        created_at -> Timestamp,
    }
}

table! {
    msp_clients (org_uuid) {
        org_uuid -> Text,
        msp_uuid -> Text,
        created_at -> Timestamp,
        preset -> Nullable<Text>,
        enrollment_token -> Nullable<Text>,
        enrollment_token_created_at -> Nullable<Timestamp>,
        everyone_collection -> Nullable<Text>,
    }
}

table! {
    msp_enrollment_joins (membership_uuid) {
        membership_uuid -> Text,
        org_uuid -> Text,
        joined_at -> Timestamp,
    }
}

table! {
    msp_memberships (membership_uuid) {
        membership_uuid -> Text,
        msp_uuid -> Text,
    }
}

table! {
    msp_events (uuid) {
        uuid -> Text,
        msp_uuid -> Text,
        created_at -> Timestamp,
        event -> Text,
        actor_uuid -> Nullable<Text>,
        actor_email -> Nullable<Text>,
        org_uuid -> Nullable<Text>,
        org_name -> Nullable<Text>,
        target_email -> Nullable<Text>,
        detail -> Nullable<Text>,
        ip -> Nullable<Text>,
    }
}

table! {
    msp_invites (uuid) {
        uuid -> Text,
        msp_uuid -> Text,
        email -> Text,
        role -> Integer,
        token -> Text,
        invited_by_email -> Text,
        created_at -> Timestamp,
    }
}

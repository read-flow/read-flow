// @generated automatically by Diesel CLI.

diesel::table! {
    api_keys (id) {
        id -> Integer,
        key -> Text,
        name -> Text,
        user_id -> Integer,
        scopes -> Text,
        created_at -> Timestamp,
        expires_at -> Nullable<Timestamp>,
        last_used -> Nullable<Timestamp>,
    }
}

diesel::table! {
    directories (id) {
        id -> Integer,
        path -> Text,
        #[sql_name = "type"]
        type_ -> Text,
    }
}

diesel::table! {
    directory_tags (directory_id, tag) {
        directory_id -> Integer,
        tag -> Text,
    }
}

diesel::table! {
    file_tags (file_id, tag) {
        file_id -> Integer,
        tag -> Text,
    }
}

diesel::table! {
    files (id) {
        id -> Integer,
        path -> Text,
        #[sql_name = "type"]
        type_ -> Text,
        size -> Integer,
        fingerprint -> Text,
        status -> Integer,
    }
}

diesel::table! {
    remotes (id) {
        id -> Integer,
        base_url -> Text,
    }
}

diesel::table! {
    users (id) {
        id -> Integer,
        username -> Text,
        password_hash -> Text,
        email -> Nullable<Text>,
        role -> Text,
        created_at -> Timestamp,
        last_login -> Nullable<Timestamp>,
    }
}

diesel::joinable!(api_keys -> users (user_id));
diesel::joinable!(directory_tags -> directories (directory_id));
diesel::joinable!(file_tags -> files (file_id));

diesel::allow_tables_to_appear_in_same_query!(
    api_keys,
    directories,
    directory_tags,
    file_tags,
    files,
    remotes,
    users,
);

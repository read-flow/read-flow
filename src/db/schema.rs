// @generated automatically by Diesel CLI.

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

diesel::joinable!(directory_tags -> directories (directory_id));
diesel::joinable!(file_tags -> files (file_id));

diesel::allow_tables_to_appear_in_same_query!(
    directories,
    directory_tags,
    file_tags,
    files,
    remotes,
);

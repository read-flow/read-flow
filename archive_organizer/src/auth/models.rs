use std::fmt;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::db::schema::{api_keys, users};

// Helper function to convert string to Role enum
pub fn from_str_role(s: &str) -> Option<Role> {
    match s.to_lowercase().as_str() {
        "admin" => Some(Role::Admin),
        "write" => Some(Role::Write),
        "read" => Some(Role::Read),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Admin,
    Write,
    Read,
}

// Implement Display for Role to enable string formatting
impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Role {
    pub fn from_name(s: &str) -> Option<Self> {
        from_str_role(s)
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Write => "write",
            Role::Read => "read",
        }
    }

    pub fn can_read(&self) -> bool {
        true // All roles can read
    }

    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::Write)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

pub type Scope = Role; // Reuse the same permissions model for API keys

#[derive(Queryable, Identifiable, Serialize, Deserialize, Debug, Clone)]
#[diesel(table_name = users)]
pub struct User {
    pub id: i32,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub email: Option<String>,
    pub role: String, // Stored as string in DB
    pub created_at: NaiveDateTime,
    pub last_login: Option<NaiveDateTime>,
}

impl User {
    pub fn role(&self) -> Role {
        Role::from_name(&self.role).unwrap_or(Role::Read)
    }
}

#[derive(Insertable, Debug)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub role: String,
}

#[derive(Queryable, Identifiable, Associations, Serialize, Deserialize, Debug, Clone)]
#[diesel(belongs_to(User))]
#[diesel(table_name = api_keys)]
pub struct ApiKey {
    pub id: i32,
    pub key: String,
    pub name: String,
    pub user_id: i32,
    pub scopes: String, // Stored as JSON array of strings
    pub created_at: NaiveDateTime,
    pub expires_at: Option<NaiveDateTime>,
    pub last_used: Option<NaiveDateTime>,
}

impl ApiKey {
    pub fn has_scope(&self, scope: &Scope) -> bool {
        // Parse JSON array of scopes
        if let Ok(scopes) = serde_json::from_str::<Vec<String>>(&self.scopes) {
            scopes.contains(&scope.to_str().to_string())
        } else {
            false
        }
    }

    pub fn scopes_vec(&self) -> Vec<String> {
        serde_json::from_str(&self.scopes).unwrap_or_else(|_| vec![])
    }
}

#[derive(Insertable, Debug)]
#[diesel(table_name = api_keys)]
pub struct NewApiKey {
    pub key: String,
    pub name: String,
    pub user_id: i32,
    pub scopes: String, // JSON array of strings
}

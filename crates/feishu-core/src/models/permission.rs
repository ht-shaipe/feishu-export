//! Permission API models

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Permission member
// ─────────────────────────────────────────────────────────────────────────────

/// 权限成员类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberType {
    Email,
    Openid,
    Userid,
    Unionid,
    Openchat,
    Opendepartmentid,
    Groupid,
    Wikispaceid,
}

impl MemberType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberType::Email => "email",
            MemberType::Openid => "openid",
            MemberType::Userid => "userid",
            MemberType::Unionid => "unionid",
            MemberType::Openchat => "openchat",
            MemberType::Opendepartmentid => "opendepartmentid",
            MemberType::Groupid => "groupid",
            MemberType::Wikispaceid => "wikispaceid",
        }
    }
}

impl std::fmt::Display for MemberType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 权限级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perm {
    View,
    Edit,
    FullAccess,
}

impl Perm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Perm::View => "view",
            Perm::Edit => "edit",
            Perm::FullAccess => "full_access",
        }
    }
}

impl std::fmt::Display for Perm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Deserialize)]
pub struct CreatePermissionResponse {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct ListPermissionResponse {
    pub code: i32,
    pub msg: String,
    pub data: ListPermissionData,
}

#[derive(Debug, Deserialize)]
pub struct ListPermissionData {
    #[serde(default)]
    pub items: Vec<PermissionMember>,
}

#[derive(Debug, Deserialize)]
pub struct PermissionMember {
    pub perm: String,
    #[serde(rename = "type")]
    pub member_type: String,
    #[serde(default)]
    pub member_id: Option<String>,
    #[serde(default)]
    pub member_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub perm_update_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeletePermissionResponse {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePermissionResponse {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchAddPermissionResponse {
    pub code: i32,
    pub msg: String,
}

#[derive(Debug, Deserialize)]
pub struct TransferOwnerResponse {
    pub code: i32,
    pub msg: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public permission
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GetPublicPermissionResponse {
    pub code: i32,
    pub msg: String,
    pub data: GetPublicPermissionData,
}

#[derive(Debug, Deserialize)]
pub struct GetPublicPermissionData {
    #[serde(default)]
    pub permission_public: Option<PermissionPublic>,
}

#[derive(Debug, Deserialize)]
pub struct PermissionPublic {
    #[serde(default)]
    pub external_access: Option<bool>,
    #[serde(default)]
    pub security_entity: Option<String>,
    #[serde(default)]
    pub comment_entity: Option<String>,
    #[serde(default)]
    pub share_entity: Option<String>,
    #[serde(default)]
    pub link_share_entity: Option<String>,
    #[serde(default)]
    pub invite_external: Option<bool>,
    #[serde(default)]
    pub share_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchPublicPermissionResponse {
    pub code: i32,
    pub msg: String,
    pub data: PatchPublicPermissionData,
}

#[derive(Debug, Deserialize)]
pub struct PatchPublicPermissionData {
    #[serde(default)]
    pub permission_public: Option<PermissionPublic>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePublicPasswordResponse {
    pub code: i32,
    pub msg: String,
    pub data: CreatePublicPasswordData,
}

#[derive(Debug, Deserialize)]
pub struct CreatePublicPasswordData {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePublicPasswordResponse {
    pub code: i32,
    pub msg: String,
    pub data: UpdatePublicPasswordData,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePublicPasswordData {
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeletePublicPasswordResponse {
    pub code: i32,
    pub msg: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth permission
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AuthPermissionResponse {
    pub code: i32,
    pub msg: String,
    pub data: AuthPermissionData,
}

#[derive(Debug, Deserialize)]
pub struct AuthPermissionData {
    #[serde(default)]
    pub has_permission: Option<bool>,
    #[serde(default)]
    pub is_supported: Option<bool>,
}

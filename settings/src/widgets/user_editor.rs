use iced::Element;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::text;
use iced::widget::text_input;
use read_flow_core::settings::HashedPassword;
use read_flow_core::settings::UserEntry;

#[derive(Debug, Clone)]
pub struct UserForm {
    pub original_id: Option<String>,
    pub user_id: String,
    pub new_password: String,
    pub owner_role: bool,
}

impl UserForm {
    pub fn new_empty() -> Self {
        Self {
            original_id: None,
            user_id: String::new(),
            new_password: String::new(),
            owner_role: false,
        }
    }

    pub fn from_entry(user_id: String, entry: &UserEntry) -> Self {
        Self {
            original_id: Some(user_id),
            user_id: String::new(),
            new_password: String::new(),
            owner_role: entry.has_role("owner"),
        }
    }

    /// Build a `UserEntry` from this form.
    /// If `new_password` is non-empty, hash it; otherwise keep existing entry's password.
    pub fn to_user_entry(&self, existing: Option<&UserEntry>) -> Result<UserEntry, String> {
        let password = if !self.new_password.is_empty() {
            HashedPassword::try_from(self.new_password.clone())
                .map_err(|e| format!("Password error: {e}"))?
        } else if let Some(existing_entry) = existing {
            existing_entry.password().clone()
        } else {
            return Err("Password is required for new users".into());
        };

        if self.owner_role {
            Ok(UserEntry::Extended {
                password,
                roles: vec!["owner".into()],
            })
        } else {
            Ok(UserEntry::Simple(password))
        }
    }
}

#[derive(Debug, Clone)]
pub enum UserFormMessage {
    UserIdChanged(String),
    PasswordChanged(String),
    OwnerRoleToggled(bool),
}

pub fn view_user_form<'a, Msg: Clone + 'a>(
    form: &'a UserForm,
    wrap: impl Fn(UserFormMessage) -> Msg + Clone + 'a,
    on_save: Msg,
    on_cancel: Msg,
) -> Element<'a, Msg> {
    let id_row: Element<'a, Msg> = if form.original_id.is_none() {
        row![
            text("User ID:").width(90),
            text_input("username", &form.user_id)
                .on_input({
                    let wrap = wrap.clone();
                    move |s| wrap(UserFormMessage::UserIdChanged(s))
                })
                .width(200),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    } else {
        text(format!(
            "User: {}",
            form.original_id.as_deref().unwrap_or("")
        ))
        .into()
    };

    let password_row = row![
        text("Password:").width(90),
        text_input("Enter new password\u{2026}", &form.new_password)
            .on_input({
                let wrap = wrap.clone();
                move |s| wrap(UserFormMessage::PasswordChanged(s))
            })
            .secure(true)
            .width(200),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let owner_row = checkbox(form.owner_role).label("Owner role").on_toggle({
        let wrap = wrap.clone();
        move |b| wrap(UserFormMessage::OwnerRoleToggled(b))
    });

    let buttons = row![
        button(text("Save"))
            .style(button::primary)
            .on_press(on_save),
        button(text("Cancel"))
            .style(button::secondary)
            .on_press(on_cancel),
    ]
    .spacing(8);

    column![id_row, password_row, owner_row, buttons]
        .spacing(8)
        .padding(12)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_without_password_errors() {
        let form = UserForm::new_empty();
        assert!(form.to_user_entry(None).is_err());
    }

    #[test]
    fn new_user_with_password_creates_simple_entry() {
        let mut form = UserForm::new_empty();
        form.new_password = "hunter2".into();
        form.owner_role = false;
        let entry = form.to_user_entry(None).unwrap();
        assert!(matches!(entry, UserEntry::Simple(_)));
        assert!(!entry.has_role("owner"));
    }

    #[test]
    fn owner_flag_creates_extended_entry() {
        let mut form = UserForm::new_empty();
        form.new_password = "hunter2".into();
        form.owner_role = true;
        let entry = form.to_user_entry(None).unwrap();
        assert!(entry.has_role("owner"));
    }
}

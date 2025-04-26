use gtk::prelude::*;
use relm4::{
    component::{AsyncComponent, AsyncComponentParts, AsyncComponentSender},
    gtk, AsyncComponentSender as Sender, RelmWidgetExt,
};
use std::sync::Arc;

use archive_organizer::{
    ApplicationModule,
    auth::{User, ApiKey, Role},
};

#[derive(Debug)]
pub struct AuthManagementDialog {
    application_module: Arc<ApplicationModule>,
    current_user: Option<User>,
    login_username: String,
    login_password: String,
    login_error: Option<String>,
    users: Vec<User>,
    api_keys: Vec<ApiKey>,
}

#[derive(Debug)]
pub enum AuthInput {
    Login,
    Logout,
    LoadUsers,
    LoadApiKeys,
    SetLoginUsername(String),
    SetLoginPassword(String),
    Close,
}

#[derive(Debug)]
pub enum AuthOutput {
    Closed,
}

#[relm4::component(async)]
impl AsyncComponent for AuthManagementDialog {
    type Init = Arc<ApplicationModule>;
    type Input = AuthInput;
    type Output = AuthOutput;
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_title: Some("Authentication Management"),
            set_default_size: (600, 400),
            set_modal: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 10,
                set_margin_all: 10,

                // Login section
                gtk::Frame {
                    set_label: Some("Login"),
                    #[watch]
                    set_visible: model.current_user.is_none(),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 10,
                        set_margin_all: 10,

                        gtk::Grid {
                            set_row_spacing: 10,
                            set_column_spacing: 10,

                            attach[0, 0, 1, 1]: &gtk::Label {
                                set_label: "Username:",
                                set_halign: gtk::Align::End,
                            },

                            #[name = "username_entry"]
                            attach[1, 0, 1, 1]: &gtk::Entry {
                                set_hexpand: true,
                                connect_changed[sender] => move |entry| {
                                    sender.input(AuthInput::SetLoginUsername(entry.text().to_string()));
                                }
                            },

                            attach[0, 1, 1, 1]: &gtk::Label {
                                set_label: "Password:",
                                set_halign: gtk::Align::End,
                            },

                            #[name = "password_entry"]
                            attach[1, 1, 1, 1]: &gtk::Entry {
                                set_visibility: false,
                                set_hexpand: true,
                                connect_changed[sender] => move |entry| {
                                    sender.input(AuthInput::SetLoginPassword(entry.text().to_string()));
                                }
                            },

                            attach[1, 2, 1, 1]: &gtk::Button {
                                set_label: "Login",
                                set_halign: gtk::Align::End,
                                connect_clicked[sender] => move |_| {
                                    sender.input(AuthInput::Login);
                                }
                            },
                        },

                        gtk::Label {
                            #[watch]
                            set_visible: model.login_error.is_some(),
                            #[watch]
                            set_label: &model.login_error.clone().unwrap_or_default(),
                            add_css_class: "error",
                        }
                    }
                },

                // User info section
                gtk::Frame {
                    set_label: Some("User Information"),
                    #[watch]
                    set_visible: model.current_user.is_some(),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 10,
                        set_margin_all: 10,

                        gtk::Label {
                            #[watch]
                            set_label: &format!("Logged in as: {}", model.current_user.as_ref().map_or("", |u| &u.username)),
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Label {
                            #[watch]
                            set_label: &format!("Role: {}", model.current_user.as_ref().map_or("", |u| &u.role)),
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Button {
                            set_label: "Logout",
                            set_halign: gtk::Align::Start,
                            connect_clicked[sender] => move |_| {
                                sender.input(AuthInput::Logout);
                            }
                        }
                    }
                },

                // Admin section
                gtk::Frame {
                    set_label: Some("Administration"),
                    #[watch]
                    set_visible: model.current_user.as_ref().map_or(false, |u| u.role() == Role::Admin),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 10,
                        set_margin_all: 10,

                        gtk::Label {
                            set_label: "User Management",
                            set_halign: gtk::Align::Start,
                            add_css_class: "heading",
                        },

                        gtk::Label {
                            set_label: "This is a placeholder for the user management interface.\nIn a future update, you'll be able to create, edit, and delete users here.",
                            set_halign: gtk::Align::Start,
                            set_margin_bottom: 20,
                        },

                        gtk::Label {
                            set_label: "API Key Management",
                            set_halign: gtk::Align::Start,
                            add_css_class: "heading",
                        },

                        gtk::Label {
                            set_label: "This is a placeholder for the API key management interface.\nIn a future update, you'll be able to create and revoke API keys here.",
                            set_halign: gtk::Align::Start,
                        }
                    }
                },

                gtk::Button {
                    set_label: "Close",
                    set_halign: gtk::Align::End,
                    set_margin_top: 10,
                    connect_clicked[sender] => move |_| {
                        sender.input(AuthInput::Close);
                    }
                }
            },

            connect_close_request[sender] => move |_| {
                sender.input(AuthInput::Close);
                gtk::Inhibit(true)
            }
        }
    }

    async fn init(
        application_module: Self::Init,
        window: Self::Root,
        sender: Sender<Self::Input>,
    ) -> AsyncComponentParts<Self> {
        let model = AuthManagementDialog {
            application_module,
            current_user: None,
            login_username: String::new(),
            login_password: String::new(),
            login_error: None,
            users: Vec::new(),
            api_keys: Vec::new(),
        };

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: Sender<Self::Input>,
        root: &Self::Root,
    ) {
        match msg {
            AuthInput::Login => {
                self.login_error = None;
                
                if self.login_username.is_empty() || self.login_password.is_empty() {
                    self.login_error = Some("Username and password are required".to_string());
                    return;
                }
                
                match self.application_module.auth_service().authenticate(&self.login_username, &self.login_password).await {
                    Ok(user) => {
                        self.current_user = Some(user);
                        
                        // Load data
                        sender.input(AuthInput::LoadUsers);
                        sender.input(AuthInput::LoadApiKeys);
                        
                        // Clear login fields
                        self.login_username = String::new();
                        self.login_password = String::new();
                    },
                    Err(_) => {
                        self.login_error = Some("Invalid username or password".to_string());
                    }
                }
            },
            AuthInput::Logout => {
                self.current_user = None;
                self.users.clear();
                self.api_keys.clear();
            },
            AuthInput::LoadUsers => {
                if let Some(user) = &self.current_user {
                    if user.role() == Role::Admin {
                        // Only admins can list all users
                        match self.application_module.auth_service().list_users(user.id).await {
                            Ok(users) => {
                                self.users = users;
                            },
                            Err(e) => {
                                eprintln!("Error loading users: {}", e);
                            }
                        }
                    }
                }
            },
            AuthInput::LoadApiKeys => {
                if let Some(user) = &self.current_user {
                    match self.application_module.auth_service().list_api_keys(user.id).await {
                        Ok(api_keys) => {
                            self.api_keys = api_keys;
                        },
                        Err(e) => {
                            eprintln!("Error loading API keys: {}", e);
                        }
                    }
                }
            },
            AuthInput::SetLoginUsername(username) => {
                self.login_username = username;
            },
            AuthInput::SetLoginPassword(password) => {
                self.login_password = password;
            },
            AuthInput::Close => {
                sender.output(AuthOutput::Closed);
                root.close();
            }
        }
    }
}

use glib::Propagation;
use gtk::prelude::*;
use relm4::{
    RelmWidgetExt,
    component::{AsyncComponent, AsyncComponentParts, AsyncComponentSender},
    gtk,
};

use archive_organizer::{
    ApplicationModule,
    auth::{ApiKey, Role, User},
};

#[derive(Debug)]
pub struct AuthManagementDialog {
    application_module: ApplicationModule,
    current_user: Option<User>,
    // Login form
    login_username: String,
    login_password: String,
    login_error: Option<String>,
    // Data
    users: Vec<User>,
    api_keys: Vec<ApiKey>,
    // New user form
    new_username: String,
    new_password: String,
    new_email: String,
    new_role: Role,
    new_user_error: Option<String>,
    // New API key form
    new_api_key_name: String,
    new_api_key_scopes: Vec<String>,
    new_api_key_error: Option<String>,
    new_api_key_result: Option<String>,
}

#[derive(Debug)]
pub enum AuthInput {
    Login,
    Logout,
    LoadUsers,
    LoadApiKeys,
    SetLoginUsername(String),
    SetLoginPassword(String),
    // User management
    SetNewUsername(String),
    SetNewPassword(String),
    SetNewEmail(String),
    SetNewRole(Role),
    CreateUser,
    UpdateUserRole(i32, Role),
    // API key management
    SetNewApiKeyName(String),
    SetNewApiKeyScopes(Vec<String>),
    CreateApiKey,
    DeleteApiKey(i32),
    Close,
}

#[derive(Debug)]
pub enum AuthOutput {
    Closed,
}

#[relm4::component(pub, async)]
impl AsyncComponent for AuthManagementDialog {
    type Init = ApplicationModule;
    type Input = AuthInput;
    type Output = AuthOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Dialog {
            set_title: Some("Authentication Management"),
            set_default_size: (600, 400),
            set_modal: true,
            set_hide_on_close: true,
            present: (),

            #[wrap(Some)]
            set_child = &gtk::Box {
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

                            attach[0, 0, 1, 1] = &gtk::Label {
                                set_label: "Username:",
                                set_halign: gtk::Align::End,
                            },

                            #[name = "username_entry"]
                            attach[1, 0, 1, 1] = &gtk::Entry {
                                set_hexpand: true,
                                connect_changed[sender] => move |entry| {
                                    sender.input(AuthInput::SetLoginUsername(entry.text().into()));
                                }
                            },

                            attach[0, 1, 1, 1] = &gtk::Label {
                                set_label: "Password:",
                                set_halign: gtk::Align::End,
                            },

                            #[name = "password_entry"]
                            attach[1, 1, 1, 1] = &gtk::Entry {
                                set_visibility: false,
                                set_hexpand: true,
                                connect_changed[sender] => move |entry| {
                                    sender.input(AuthInput::SetLoginPassword(entry.text().into()));
                                }
                            },

                            attach[1, 2, 1, 1] = &gtk::Button {
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
                            set_label: &format!("Role: {}", model.current_user.as_ref().map_or("", |u| u.role().to_str())),
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
                    set_visible: model.current_user.as_ref().is_some_and(|u| u.role() == Role::Admin),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 10,
                        set_margin_all: 10,

                        gtk::Label {
                            set_label: "User Management",
                            set_halign: gtk::Align::Start,
                            add_css_class: "heading",
                        },

                        // User list
                        gtk::ScrolledWindow {
                            set_min_content_height: 150,
                            set_vexpand: false,
                            set_margin_bottom: 10,

                            gtk::ListView {
                                set_factory: Some(&{
                                    let factory = gtk::SignalListItemFactory::new();
                                    let sender_clone = sender.input_sender().clone();

                                    factory.connect_setup(move |_, list_item| {
                                        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 5);
                                        box_.set_margin_all(5);

                                        let username_label = gtk::Label::new(None);
                                        username_label.set_hexpand(true);
                                        username_label.set_halign(gtk::Align::Start);
                                        box_.append(&username_label);

                                        let role_label = gtk::Label::new(None);
                                        role_label.set_width_chars(10);
                                        box_.append(&role_label);

                                        let change_role_button = gtk::Button::new();
                                        change_role_button.set_icon_name("document-edit-symbolic");
                                        box_.append(&change_role_button);

                                        list_item.set_child(Some(&box_));
                                    });

                                    factory.connect_bind(move |_, list_item| {
                                        let box_ = list_item.child().and_downcast::<gtk::Box>().unwrap();
                                        let username_label = box_.first_child().and_downcast::<gtk::Label>().unwrap();
                                        let role_label = username_label.next_sibling().and_downcast::<gtk::Label>().unwrap();
                                        let change_role_button = role_label.next_sibling().and_downcast::<gtk::Button>().unwrap();

                                        let user_obj = list_item.item().and_downcast::<gtk::StringObject>().unwrap();
                                        let user_str = user_obj.string();
                                        tracing::debug!("User JSON: {}", user_str);
                                        let user_data: User = match serde_json::from_str(&user_str) {
                                            Ok(user) => user,
                                            Err(e) => {
                                                tracing::error!("Failed to deserialize user: {}", e);
                                                return;
                                            }
                                        };

                                        username_label.set_label(&user_data.username);
                                        role_label.set_label(&user_data.role);

                                        let sender_clone = sender_clone.clone();
                                        let user_id = user_data.id;
                                        let current_role = user_data.role().clone();

                                        // Toggle between Admin and Read roles
                                        change_role_button.connect_clicked(move |_| {
                                            let new_role = if current_role == Role::Admin {
                                                Role::Read
                                            } else {
                                                Role::Admin
                                            };
                                            sender_clone.send(AuthInput::UpdateUserRole(user_id, new_role)).unwrap();
                                        });
                                    });

                                    factory
                                }),
                                #[watch]
                                set_model: Some(&{
                                    let string_list = gtk::StringList::new(&[]);
                                    for user in &model.users {
                                        // Create a simplified JSON representation without password_hash
                                        let user_json = serde_json::json!({
                                            "id": user.id,
                                            "username": user.username,
                                            "email": user.email,
                                            "role": user.role,
                                            "created_at": user.created_at,
                                            "last_login": user.last_login
                                        }).to_string();
                                        string_list.append(&user_json);
                                    }
                                    gtk::NoSelection::new(Some(string_list))
                                }),
                            }
                        },

                        // New user form
                        gtk::Frame {
                            set_label: Some("Create New User"),

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 10,
                                set_margin_all: 10,

                                gtk::Grid {
                                    set_row_spacing: 5,
                                    set_column_spacing: 10,
                                    set_margin_bottom: 10,

                                    attach[0, 0, 1, 1] = &gtk::Label {
                                        set_label: "Username:",
                                        set_halign: gtk::Align::End,
                                    },

                                    attach[1, 0, 1, 1] = &gtk::Entry {
                                        set_hexpand: true,
                                        #[watch]
                                        set_text: &model.new_username,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(AuthInput::SetNewUsername(entry.text().into()));
                                        }
                                    },

                                    attach[0, 1, 1, 1] = &gtk::Label {
                                        set_label: "Password:",
                                        set_halign: gtk::Align::End,
                                    },

                                    attach[1, 1, 1, 1] = &gtk::Entry {
                                        set_hexpand: true,
                                        set_visibility: false,
                                        #[watch]
                                        set_text: &model.new_password,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(AuthInput::SetNewPassword(entry.text().into()));
                                        }
                                    },

                                    attach[0, 2, 1, 1] = &gtk::Label {
                                        set_label: "Email (optional):",
                                        set_halign: gtk::Align::End,
                                    },

                                    attach[1, 2, 1, 1] = &gtk::Entry {
                                        set_hexpand: true,
                                        #[watch]
                                        set_text: &model.new_email,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(AuthInput::SetNewEmail(entry.text().into()));
                                        }
                                    },

                                    attach[0, 3, 1, 1] = &gtk::Label {
                                        set_label: "Role:",
                                        set_halign: gtk::Align::End,
                                    },

                                    attach[1, 3, 1, 1] = &gtk::DropDown::from_strings(&["Read", "Write", "Admin"]) {
                                        #[watch]
                                        set_selected: match model.new_role {
                                            Role::Read => 0,
                                            Role::Write => 1,
                                            Role::Admin => 2,
                                        },
                                        connect_selected_notify[sender] => move |dropdown| {
                                            let role = match dropdown.selected() {
                                                0 => Role::Read,
                                                1 => Role::Write,
                                                2 => Role::Admin,
                                                _ => Role::Read,
                                            };
                                            sender.input(AuthInput::SetNewRole(role));
                                        }
                                    },
                                },

                                gtk::Label {
                                    #[watch]
                                    set_visible: model.new_user_error.is_some(),
                                    #[watch]
                                    set_label: &model.new_user_error.clone().unwrap_or_default(),
                                    add_css_class: "error",
                                    set_margin_bottom: 10,
                                },

                                gtk::Button {
                                    set_label: "Create User",
                                    add_css_class: "suggested-action",
                                    set_halign: gtk::Align::End,
                                    connect_clicked[sender] => move |_| {
                                        sender.input(AuthInput::CreateUser);
                                    }
                                }
                            }
                        },

                        set_margin_bottom: 20,

                        gtk::Label {
                            set_label: "API Key Management",
                            set_halign: gtk::Align::Start,
                            add_css_class: "heading",
                        },

                        // API key list
                        gtk::ScrolledWindow {
                            set_min_content_height: 150,
                            set_vexpand: false,
                            set_margin_bottom: 10,

                            gtk::ListView {
                                set_factory: Some(&{
                                    let factory = gtk::SignalListItemFactory::new();
                                    let sender_clone = sender.input_sender().clone();

                                    factory.connect_setup(move |_, list_item| {
                                        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 5);
                                        box_.set_margin_all(5);

                                        let name_label = gtk::Label::new(None);
                                        name_label.set_hexpand(true);
                                        name_label.set_halign(gtk::Align::Start);
                                        box_.append(&name_label);

                                        let created_label = gtk::Label::new(None);
                                        created_label.set_width_chars(20);
                                        box_.append(&created_label);

                                        let delete_button = gtk::Button::new();
                                        delete_button.set_icon_name("user-trash-symbolic");
                                        delete_button.add_css_class("destructive-action");
                                        box_.append(&delete_button);

                                        list_item.set_child(Some(&box_));
                                    });

                                    factory.connect_bind(move |_, list_item| {
                                        let box_ = list_item.child().and_downcast::<gtk::Box>().unwrap();
                                        let name_label = box_.first_child().and_downcast::<gtk::Label>().unwrap();
                                        let created_label = name_label.next_sibling().and_downcast::<gtk::Label>().unwrap();
                                        let delete_button = created_label.next_sibling().and_downcast::<gtk::Button>().unwrap();

                                        let api_key_obj = list_item.item().and_downcast::<gtk::StringObject>().unwrap();
                                        let api_key_str = api_key_obj.string();
                                        tracing::debug!("API Key JSON: {}", api_key_str);
                                        let api_key_data: ApiKey = match serde_json::from_str(&api_key_str) {
                                            Ok(api_key) => api_key,
                                            Err(e) => {
                                                tracing::error!("Failed to deserialize API key: {}", e);
                                                return;
                                            }
                                        };

                                        name_label.set_label(&api_key_data.name);
                                        created_label.set_label(&api_key_data.created_at.format("%Y-%m-%d %H:%M").to_string());

                                        let sender_clone = sender_clone.clone();
                                        let key_id = api_key_data.id;
                                        delete_button.connect_clicked(move |_| {
                                            sender_clone.send(AuthInput::DeleteApiKey(key_id)).unwrap();
                                        });
                                    });

                                    factory
                                }),
                                #[watch]
                                set_model: Some(&{
                                    let string_list = gtk::StringList::new(&[]);
                                    for api_key in &model.api_keys {
                                        // Create a simplified JSON representation
                                        let api_key_json = serde_json::json!({
                                            "id": api_key.id,
                                            "name": api_key.name,
                                            "user_id": api_key.user_id,
                                            "created_at": api_key.created_at,
                                            "last_used": api_key.last_used
                                        }).to_string();
                                        string_list.append(&api_key_json);
                                    }
                                    gtk::NoSelection::new(Some(string_list))
                                }),
                            }
                        },

                        // New API key form
                        gtk::Frame {
                            set_label: Some("Create New API Key"),

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 10,
                                set_margin_all: 10,

                                gtk::Grid {
                                    set_row_spacing: 5,
                                    set_column_spacing: 10,
                                    set_margin_bottom: 10,

                                    attach[0, 0, 1, 1] = &gtk::Label {
                                        set_label: "Name:",
                                        set_halign: gtk::Align::End,
                                    },

                                    attach[1, 0, 1, 1] = &gtk::Entry {
                                        set_hexpand: true,
                                        #[watch]
                                        set_text: &model.new_api_key_name,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(AuthInput::SetNewApiKeyName(entry.text().into()));
                                        }
                                    },
                                },

                                gtk::Label {
                                    #[watch]
                                    set_visible: model.new_api_key_error.is_some(),
                                    #[watch]
                                    set_label: &model.new_api_key_error.clone().unwrap_or_default(),
                                    add_css_class: "error",
                                    set_margin_bottom: 10,
                                },

                                gtk::Label {
                                    #[watch]
                                    set_visible: model.new_api_key_result.is_some(),
                                    #[watch]
                                    set_markup: &format!("<b>API Key:</b> {}", model.new_api_key_result.clone().unwrap_or_default()),
                                    set_selectable: true,
                                    add_css_class: "success",
                                    set_margin_bottom: 10,
                                },

                                gtk::Button {
                                    set_label: "Create API Key",
                                    add_css_class: "suggested-action",
                                    set_halign: gtk::Align::End,
                                    connect_clicked[sender] => move |_| {
                                        sender.input(AuthInput::CreateApiKey);
                                    }
                                }
                            }
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
                Propagation::Stop
            }
        }
    }

    async fn init(
        application_module: Self::Init,
        window: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        tracing::info!("Initializing AuthManagementDialog");

        // Create a default admin user if none exists
        Self::create_default_admin_if_needed(&application_module).await;

        let model = AuthManagementDialog {
            application_module,
            current_user: None,
            // Login form
            login_username: String::new(),
            login_password: String::new(),
            login_error: None,
            // Data
            users: Vec::new(),
            api_keys: Vec::new(),
            // New user form
            new_username: String::new(),
            new_password: String::new(),
            new_email: String::new(),
            new_role: Role::Read,
            new_user_error: None,
            // New API key form
            new_api_key_name: String::new(),
            new_api_key_scopes: vec![],
            new_api_key_error: None,
            new_api_key_result: None,
        };

        let widgets = view_output!();
        tracing::info!("AuthManagementDialog widgets created");

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            AuthInput::Login => {
                self.login_error = None;

                if self.login_username.is_empty() || self.login_password.is_empty() {
                    self.login_error = Some("Username and password are required".to_string());
                    return;
                }

                match self
                    .application_module
                    .auth_service()
                    .authenticate(&self.login_username, &self.login_password)
                    .await
                {
                    Ok(user) => {
                        self.current_user = Some(user);

                        // Load data
                        sender.input(AuthInput::LoadUsers);
                        sender.input(AuthInput::LoadApiKeys);

                        // Clear login fields
                        self.login_username = String::new();
                        self.login_password = String::new();
                    }
                    Err(_) => {
                        self.login_error = Some("Invalid username or password".to_string());
                    }
                }
            }
            AuthInput::Logout => {
                self.current_user = None;
                self.users.clear();
                self.api_keys.clear();
            }
            AuthInput::LoadUsers => {
                if let Some(user) = &self.current_user {
                    if user.role() == Role::Admin {
                        // Only admins can list all users
                        match self
                            .application_module
                            .auth_service()
                            .list_users(user.id)
                            .await
                        {
                            Ok(users) => {
                                self.users = users;
                            }
                            Err(e) => {
                                eprintln!("Error loading users: {}", e);
                            }
                        }
                    }
                }
            }
            AuthInput::LoadApiKeys => {
                if let Some(user) = &self.current_user {
                    match self
                        .application_module
                        .auth_service()
                        .list_api_keys(user.id)
                        .await
                    {
                        Ok(api_keys) => {
                            self.api_keys = api_keys;
                        }
                        Err(e) => {
                            eprintln!("Error loading API keys: {}", e);
                        }
                    }
                }
            }
            AuthInput::SetLoginUsername(username) => {
                self.login_username = username;
            }
            AuthInput::SetLoginPassword(password) => {
                self.login_password = password;
            }
            // User management
            AuthInput::SetNewUsername(username) => {
                self.new_username = username;
            }
            AuthInput::SetNewPassword(password) => {
                self.new_password = password;
            }
            AuthInput::SetNewEmail(email) => {
                self.new_email = email;
            }
            AuthInput::SetNewRole(role) => {
                self.new_role = role;
            }
            AuthInput::CreateUser => {
                self.new_user_error = None;

                // Validate inputs
                if self.new_username.is_empty() || self.new_password.is_empty() {
                    self.new_user_error = Some("Username and password are required".to_string());
                    return;
                }

                // Create the user
                if let Some(current_user) = &self.current_user {
                    if current_user.role() == Role::Admin {
                        let email = if self.new_email.is_empty() {
                            None
                        } else {
                            Some(self.new_email.as_str())
                        };

                        match self
                            .application_module
                            .auth_service()
                            .register_user(
                                &self.new_username,
                                &self.new_password,
                                email,
                                self.new_role.clone(),
                            )
                            .await
                        {
                            Ok(_) => {
                                // Clear form and reload users
                                self.new_username = String::new();
                                self.new_password = String::new();
                                self.new_email = String::new();
                                self.new_role = Role::Read;
                                sender.input(AuthInput::LoadUsers);
                            }
                            Err(e) => {
                                self.new_user_error = Some(format!("Error creating user: {}", e));
                            }
                        }
                    }
                }
            }
            AuthInput::UpdateUserRole(user_id, new_role) => {
                if let Some(current_user) = &self.current_user {
                    if current_user.role() == Role::Admin {
                        // Don't allow changing your own role
                        if current_user.id == user_id {
                            self.new_user_error = Some("Cannot change your own role".to_string());
                            return;
                        }

                        // Update the user's role
                        match self
                            .application_module
                            .auth_service()
                            .update_user_role(user_id, new_role, current_user.id)
                            .await
                        {
                            Ok(_) => {
                                // Reload users
                                sender.input(AuthInput::LoadUsers);
                            }
                            Err(e) => {
                                self.new_user_error =
                                    Some(format!("Error updating user role: {}", e));
                            }
                        }
                    }
                }
            }

            // API key management
            AuthInput::SetNewApiKeyName(name) => {
                self.new_api_key_name = name;
            }
            AuthInput::SetNewApiKeyScopes(scopes) => {
                self.new_api_key_scopes = scopes;
            }
            AuthInput::CreateApiKey => {
                self.new_api_key_error = None;
                self.new_api_key_result = None;

                // Validate inputs
                if self.new_api_key_name.is_empty() {
                    self.new_api_key_error = Some("API key name is required".to_string());
                    return;
                }

                // Create the API key
                if let Some(current_user) = &self.current_user {
                    // Convert string scopes to Role enum
                    let scopes = self
                        .new_api_key_scopes
                        .iter()
                        .filter_map(|s| Role::from_str(s))
                        .collect::<Vec<_>>();

                    match self
                        .application_module
                        .auth_service()
                        .create_api_key(current_user.id, &self.new_api_key_name, scopes)
                        .await
                    {
                        Ok(api_key) => {
                            // Show the API key to the user (this is the only time they'll see it)
                            self.new_api_key_result = Some(api_key.key.clone());

                            // Clear form and reload API keys
                            self.new_api_key_name = String::new();
                            self.new_api_key_scopes = vec![];
                            sender.input(AuthInput::LoadApiKeys);
                        }
                        Err(e) => {
                            self.new_api_key_error = Some(format!("Error creating API key: {}", e));
                        }
                    }
                }
            }
            AuthInput::DeleteApiKey(key_id) => {
                if let Some(current_user) = &self.current_user {
                    match self
                        .application_module
                        .auth_service()
                        .delete_api_key(key_id, current_user.id)
                        .await
                    {
                        Ok(_) => {
                            // Reload API keys
                            sender.input(AuthInput::LoadApiKeys);
                        }
                        Err(e) => {
                            self.new_api_key_error = Some(format!("Error deleting API key: {}", e));
                        }
                    }
                }
            }

            AuthInput::Close => {
                tracing::info!("Closing AuthManagementDialog");
                let _ = sender.output(AuthOutput::Closed);
                root.hide();
            }
        }
    }
}

impl AuthManagementDialog {
    async fn create_default_admin_if_needed(application_module: &ApplicationModule) {
        // Check if any users exist by trying to authenticate with default credentials
        let auth_service = application_module.auth_service();

        // Try to authenticate with default admin credentials
        match auth_service.authenticate("admin", "admin").await {
            Ok(_) => {
                // Admin user exists and credentials are valid
                tracing::info!("Default admin user already exists");
            }
            Err(_) => {
                // Either admin doesn't exist or credentials are wrong
                // Let's try to create a default admin user
                tracing::info!("Attempting to create default admin user");
                match auth_service
                    .register_user("admin", "admin", None, Role::Admin)
                    .await
                {
                    Ok(_) => tracing::info!(
                        "Created default admin user with username 'admin' and password 'admin'"
                    ),
                    Err(e) => {
                        if e.to_string().contains("already exists") {
                            tracing::info!("Admin user already exists but with different password");
                        } else {
                            tracing::error!("Failed to create default admin user: {}", e);
                        }
                    }
                }
            }
        }
    }
}

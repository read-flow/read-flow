# Authentication and Authorization in Archive Organizer

This document describes the authentication and authorization system in Archive Organizer.

## Overview

Archive Organizer uses a hybrid authentication system that supports:

1. **JWT (JSON Web Token)** - For web and API clients
2. **API Keys** - For programmatic access
3. **Legacy Tokens** - For backward compatibility

## User Roles

The system supports three roles with different permission levels:

- **Admin** - Full access to all features, including user management
- **Write** - Can read and modify content
- **Read** - Can only read content

## Authentication Methods

### JWT Authentication

JWT authentication is the primary method for web clients. It provides:

- Stateless authentication
- User identity and role information
- Automatic expiration

To authenticate with JWT:

1. Send a POST request to `/auth/login` with username and password
2. Receive a JWT token
3. Include the token in the `Authorization` header as `Bearer <token>`

Example:
```bash
# Login to get a token
curl -X POST http://localhost:8000/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin"}'

# Use the token
curl http://localhost:8000/files \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
```

### API Key Authentication

API keys provide a way for applications to authenticate without user credentials. They:

- Support specific permission scopes
- Can be created and revoked independently
- Are tied to a specific user

To use API keys:

1. Create an API key through the UI or API
2. Include the key in the `Authorization` header as `Bearer <api-key>`

Example:
```bash
# Create an API key (requires authentication)
curl -X POST http://localhost:8000/auth/api-keys \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  -H "Content-Type: application/json" \
  -d '{"name":"My API Key","scopes":["read"]}'

# Use the API key
curl http://localhost:8000/files \
  -H "Authorization: Bearer abcdef123456..."
```

### Legacy Token Authentication

For backward compatibility, the system still supports the legacy token authentication method:

- Tokens are defined in the configuration file
- All legacy tokens have admin privileges
- No user identity is associated with tokens

## User Management

### Default Admin User

On first startup, the system automatically creates a default admin user:

- Username: `admin`
- Password: `admin`

**Important**: Change this password immediately after first login!

### Creating Users

Only admin users can create new users. Users can be created through:

1. The GTK application's Authentication Management screen
2. The API endpoint `/auth/register`

Example:
```bash
# Register a new user (requires admin authentication)
curl -X POST http://localhost:8000/auth/register \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..." \
  -H "Content-Type: application/json" \
  -d '{"username":"newuser","password":"password123","email":"user@example.com"}'
```

### Managing API Keys

Users can create and manage their own API keys through:

1. The GTK application's Authentication Management screen
2. The API endpoints `/auth/api-keys`

## Configuration

Authentication settings are configured in the `archive-organizer.toml` file:

```toml
[server]
# Legacy authorization tokens (for backward compatibility)
authorization_tokens = ["your-secret-token"]

# JWT secret for signing tokens
jwt_secret = "your-custom-jwt-secret"
```

## Security Recommendations

1. Change the default admin password immediately
2. Use a strong, unique JWT secret
3. Regularly rotate API keys
4. Use HTTPS in production environments
5. Limit admin users to trusted individuals

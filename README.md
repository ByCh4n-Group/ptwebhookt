# PTWebhook

Modern Discord webhook TUI application built with Rust and Ratatui.

## Features

- 🎨 Modern Terminal User Interface (TUI)
- 📝 TOML-based template system
- 🚀 Easy to use
- ⚡ Async webhook sending
- 🎯 Form validation
- 📱 Responsive design
- 🌐 Multi-format URL support

## Installation

```bash
cargo build --release
```

## Usage

```bash
# Run with Discord webhook URL (full URL)
./target/release/ptwebhook --token "https://discord.com/api/webhooks/YOUR_ID/YOUR_TOKEN"

# Run with ID/TOKEN only
./target/release/ptwebhook --token "YOUR_ID/YOUR_TOKEN"

# Short version
./target/release/ptwebhook -t "YOUR_ID/YOUR_TOKEN"
```

## Supported URL Formats

- `https://discord.com/api/webhooks/ID/TOKEN`
- `discord.com/api/webhooks/ID/TOKEN`
- `ID/TOKEN`

## Controls

### Template Selection
- `↑/↓` or `j/k`: Navigate between templates
- `Enter` or `Space`: Select template
- `q` or `Esc`: Exit

### Form Filling
- `↑/↓` or `Tab/Shift+Tab`: Navigate between fields
- `Type`: Edit field
- `Backspace`: Delete character
- `Enter`: Go to preview screen
- `Esc`: Return to template selection
- `q`: Exit

### Preview
- `Enter` or `Space`: Send message to Discord
- `Esc`: Return to form filling screen
- `q`: Exit

### Result Screen
- `Enter`, `Space` or `Esc`: Return to template selection
- `q`: Exit

## Template System

Templates are stored in TOML format in the `templates/` folder.

### Example Template

```toml
[template]
name = "Announcement"
description = "General announcement template"

[fields]
title = { type = "text", label = "Title", placeholder = "Important announcement title", required = true }
content = { type = "textarea", label = "Content", placeholder = "Write your announcement content here...", required = true }
priority = { type = "select", label = "Priority", options = ["Low", "Medium", "High"], default = "Medium" }

[webhook]
username = "Announcement Bot"
avatar_url = ""
color = 5814783  # Blue color
```

### Field Types

- `text`: Single line text
- `textarea`: Multi-line text
- `select`: Option list

### Field Properties

- `label`: Field label
- `placeholder`: Placeholder text
- `required`: Required field (true/false)
- `options`: Options for select type
- `default`: Default value

## Webhook Settings

Each template contains its own webhook settings:

- `username`: Bot username
- `avatar_url`: Bot avatar URL
- `color`: Embed color (decimal)

## Development

```bash
# Run in development mode
cargo run -- --token "YOUR_ID/YOUR_TOKEN"

# Run tests
cargo test

# Format code
cargo fmt

# Linting
cargo clippy
```

## Dependencies

- `ratatui`: Terminal UI framework
- `crossterm`: Cross-platform terminal
- `tokio`: Async runtime
- `reqwest`: HTTP client
- `serde`: Serialization
- `toml`: TOML parsing
- `clap`: CLI argument parsing
- `anyhow`: Error handling
- `url`: URL parsing
- `regex`: Regular expressions

## Error Handling

The application provides detailed error messages for:
- Connection timeouts
- Invalid webhook URLs
- Network connectivity issues
- Discord API errors
- Template parsing errors

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the terms specified in the LICENSE file.

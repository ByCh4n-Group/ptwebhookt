use anyhow::{anyhow, Result};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
    },
    Frame, Terminal,
};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io,
    path::Path,
    time::Duration,
};
use tokio::time::sleep;
use url::Url;

#[derive(Parser)]
#[command(name = "ptwebhook")]
#[command(about = "Discord webhook TUI application")]
struct Cli {
    #[arg(short = 't', long = "token", help = "Discord webhook URL or token")]
    token: String,
}

fn parse_webhook_url(input: &str) -> Result<String> {
    // Eğer tam URL verilmişse
    if input.starts_with("https://discord.com/api/webhooks/") {
        return Ok(input.to_string());
    }
    
    // Eğer sadece ID/TOKEN formatında verilmişse
    let webhook_regex = Regex::new(r"^(\d+)/([a-zA-Z0-9_-]+)$")?;
    if webhook_regex.is_match(input) {
        return Ok(format!("https://discord.com/api/webhooks/{}", input));
    }
    
    // Eğer discord.com ile başlıyorsa ama https:// yoksa
    if input.starts_with("discord.com/api/webhooks/") {
        return Ok(format!("https://{}", input));
    }
    
    // Geçersiz format
    Err(anyhow!("Invalid webhook URL format. Supported formats:\n\
        - https://discord.com/api/webhooks/ID/TOKEN\n\
        - discord.com/api/webhooks/ID/TOKEN\n\
        - ID/TOKEN"))
}

#[derive(Debug, Deserialize)]
struct TemplateConfig {
    template: TemplateInfo,
    fields: HashMap<String, FieldConfig>,
    webhook: WebhookConfig,
}

#[derive(Debug, Deserialize)]
struct TemplateInfo {
    name: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct FieldConfig {
    #[serde(rename = "type")]
    field_type: String,
    label: String,
    placeholder: Option<String>,
    required: Option<bool>,
    options: Option<Vec<String>>,
    default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebhookConfig {
    username: Option<String>,
    avatar_url: Option<String>,
    color: Option<u32>,
}

#[derive(Debug, Serialize)]
struct DiscordWebhook {
    username: Option<String>,
    avatar_url: Option<String>,
    embeds: Vec<DiscordEmbed>,
}

#[derive(Debug, Serialize)]
struct DiscordEmbed {
    title: Option<String>,
    description: Option<String>,
    color: Option<u32>,
    fields: Vec<DiscordField>,
}

#[derive(Debug, Serialize)]
struct DiscordField {
    name: String,
    value: String,
    inline: bool,
}

#[derive(Debug)]
enum AppState {
    TemplateSelection,
    FormFilling,
    Preview,
    Sending,
    Result(bool, String),
}

struct App {
    state: AppState,
    templates: Vec<(String, TemplateConfig)>,
    selected_template: Option<usize>,
    template_list_state: ListState,
    current_field: usize,
    field_values: HashMap<String, String>,
    webhook_url: String,
}

impl App {
    fn new(webhook_url: String) -> Result<App> {
        let templates = load_templates()?;
        let mut app = App {
            state: AppState::TemplateSelection,
            templates,
            selected_template: None,
            template_list_state: ListState::default(),
            current_field: 0,
            field_values: HashMap::new(),
            webhook_url,
        };
        
        if !app.templates.is_empty() {
            app.template_list_state.select(Some(0));
        }
        
        Ok(app)
    }

    fn next_template(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        let i = match self.template_list_state.selected() {
            Some(i) => {
                if i >= self.templates.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.template_list_state.select(Some(i));
    }

    fn previous_template(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        let i = match self.template_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.templates.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.template_list_state.select(Some(i));
    }

    fn select_template(&mut self) {
        if let Some(selected) = self.template_list_state.selected() {
            self.selected_template = Some(selected);
            self.state = AppState::FormFilling;
            self.current_field = 0;
            self.field_values.clear();
            
            // Initialize default values
            let (_, template) = &self.templates[selected];
            for (field_name, field_config) in &template.fields {
                if let Some(default) = &field_config.default {
                    self.field_values.insert(field_name.clone(), default.clone());
                } else {
                    self.field_values.insert(field_name.clone(), String::new());
                }
            }
        }
    }

    fn next_field(&mut self) {
        if let Some(template_idx) = self.selected_template {
            let (_, template) = &self.templates[template_idx];
            if self.current_field < template.fields.len() - 1 {
                self.current_field += 1;
            }
        }
    }

    fn previous_field(&mut self) {
        if self.current_field > 0 {
            self.current_field -= 1;
        }
    }

    fn update_current_field(&mut self, value: String) {
        if let Some(template_idx) = self.selected_template {
            let (_, template) = &self.templates[template_idx];
            let field_names: Vec<_> = template.fields.keys().collect();
            if let Some(field_name) = field_names.get(self.current_field) {
                self.field_values.insert((*field_name).clone(), value);
            }
        }
    }

    fn get_current_field_value(&self) -> String {
        if let Some(template_idx) = self.selected_template {
            let (_, template) = &self.templates[template_idx];
            let field_names: Vec<_> = template.fields.keys().collect();
            if let Some(field_name) = field_names.get(self.current_field) {
                return self.field_values.get(*field_name).unwrap_or(&String::new()).clone();
            }
        }
        String::new()
    }

    async fn send_webhook(&mut self) -> Result<()> {
        if let Some(template_idx) = self.selected_template {
            let (_, template) = &self.templates[template_idx];
            
            // Create Discord webhook payload
            let mut fields = Vec::new();
            for (field_name, field_config) in &template.fields {
                if let Some(value) = self.field_values.get(field_name) {
                    if !value.is_empty() {
                        fields.push(DiscordField {
                            name: field_config.label.clone(),
                            value: value.clone(),
                            inline: false,
                        });
                    }
                }
            }

            let embed = DiscordEmbed {
                title: Some(template.template.name.clone()),
                description: Some(template.template.description.clone()),
                color: template.webhook.color,
                fields,
            };

            let webhook = DiscordWebhook {
                username: template.webhook.username.clone(),
                avatar_url: template.webhook.avatar_url.clone(),
                embeds: vec![embed],
            };

            // Send to Discord with better error handling
            let client = Client::builder()
                .timeout(Duration::from_secs(30))
                .user_agent("PTWebhook/1.0")
                .build()?;
            
            self.state = AppState::Sending;
            
            let response = client
                .post(&self.webhook_url)
                .header("Content-Type", "application/json")
                .json(&webhook)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        self.state = AppState::Result(true, "✅ Message sent successfully!".to_string());
                    } else {
                        let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
                        self.state = AppState::Result(false, format!("❌ HTTP {}: {}", status, error_text));
                    }
                }
                Err(e) => {
                    let error_msg = if e.is_timeout() {
                        "⏱️ Connection timeout"
                    } else if e.is_connect() {
                        "🌐 Connection error - Check your internet connection"
                    } else if e.is_request() {
                        "📨 Request format error"
                    } else {
                        "❌ Unknown connection error"
                    };
                    
                    self.state = AppState::Result(false, format!("{}: {}", error_msg, e));
                }
            }
        }
        Ok(())
    }
}

fn load_templates() -> Result<Vec<(String, TemplateConfig)>> {
    let mut templates = Vec::new();
    
    if Path::new("templates").exists() {
        for entry in fs::read_dir("templates")? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = fs::read_to_string(&path)?;
                let config: TemplateConfig = toml::from_str(&content)?;
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                templates.push((name, config));
            }
        }
    }
    
    Ok(templates)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Parse and validate webhook URL
    let webhook_url = match parse_webhook_url(&cli.token) {
        Ok(url) => url,
        Err(e) => {
            eprintln!("❌ Error: {}", e);
            std::process::exit(1);
        }
    };
    
    // Validate URL format
    if let Err(e) = Url::parse(&webhook_url) {
        eprintln!("❌ Invalid URL format: {}", e);
        std::process::exit(1);
    }
    
    println!("🚀 Starting Discord Webhook TUI...");
    println!("📡 Webhook URL: {}***", &webhook_url[..webhook_url.len().min(40)]);
    println!("✨ Loading modern interface...");
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let app = App::new(webhook_url);
    let res = run_app(&mut terminal, app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("❌ Application error: {:?}", err)
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: Result<App>,
) -> Result<()> {
    let mut app = app?;
    
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match app.state {
                AppState::TemplateSelection => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Down | KeyCode::Char('j') => app.next_template(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous_template(),
                        KeyCode::Enter | KeyCode::Char(' ') => app.select_template(),
                        _ => {}
                    }
                }
                AppState::FormFilling => {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Esc => app.state = AppState::TemplateSelection,
                        KeyCode::Down | KeyCode::Tab => app.next_field(),
                        KeyCode::Up | KeyCode::BackTab => app.previous_field(),
                        KeyCode::Enter => app.state = AppState::Preview,
                        KeyCode::Char(c) => {
                            let mut current = app.get_current_field_value();
                            current.push(c);
                            app.update_current_field(current);
                        }
                        KeyCode::Backspace => {
                            let mut current = app.get_current_field_value();
                            current.pop();
                            app.update_current_field(current);
                        }
                        _ => {}
                    }
                }
                AppState::Preview => {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Esc => app.state = AppState::FormFilling,
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            app.send_webhook().await?;
                        }
                        _ => {}
                    }
                }
                AppState::Sending => {
                    // Wait for sending to complete
                    sleep(Duration::from_millis(100)).await;
                }
                AppState::Result(_, _) => {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                            app.state = AppState::TemplateSelection
                        },
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    match &app.state {
        AppState::TemplateSelection => draw_template_selection(f, app),
        AppState::FormFilling => draw_form_filling(f, app),
        AppState::Preview => draw_preview(f, app),
        AppState::Sending => draw_sending(f),
        AppState::Result(success, message) => draw_result(f, *success, message),
    }
}

fn draw_template_selection(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let min_height = 20;
    let min_width = 80;
    
    // Responsive layout - adjust based on terminal size
    let (header_height, help_height) = if area.height < min_height {
        (3, 3)
    } else {
        (5, 4)
    };
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(if area.width < min_width { 0 } else { 1 })
        .constraints([
            Constraint::Length(header_height),
            Constraint::Min(8),
            Constraint::Length(help_height),
        ].as_ref())
        .split(area);

    // Header with fancy styling
    let header_lines = if area.height >= min_height {
        vec![
            Line::from(vec![
                Span::styled("🚀 ", Style::default().fg(Color::Blue)),
                Span::styled("Discord Webhook Manager", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("📋 ", Style::default().fg(Color::Yellow)),
                Span::styled("Select Template", Style::default().fg(Color::White)),
                Span::styled(" • ", Style::default().fg(Color::Gray)),
                Span::styled(format!("{} templates available", app.templates.len()), Style::default().fg(Color::Gray)),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("🚀 ", Style::default().fg(Color::Blue)),
                Span::styled("Webhook Manager", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" ({} templates)", app.templates.len()), Style::default().fg(Color::Gray)),
            ]),
        ]
    };
    
    let header = Paragraph::new(header_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" 🎯 Webhook Template Manager ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        );
    f.render_widget(header, chunks[0]);

    // Template list with better styling
    let items: Vec<ListItem> = app
        .templates
        .iter()
        .enumerate()
        .map(|(idx, (_name, config))| {
            let selected = app.template_list_state.selected().unwrap_or(0) == idx;
            let icon = match config.template.name.as_str() {
                name if name.contains("Announcement") => "📢",
                name if name.contains("Academy") => "🎓",
                name if name.contains("Project") => "💼",
                _ => "📄",
            };
            
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(Color::Yellow)),
                Span::styled(
                    config.template.name.clone(),
                    if selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    }
                ),
                Span::raw("  "),
                Span::styled(
                    config.template.description.clone(),
                    if selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Gray)
                    }
                ),
            ]))
        })
        .collect();

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(" 📚 Templates ")
        .title_style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD));

    let items = List::new(items)
        .block(list_block)
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol("  → ");

    f.render_stateful_widget(items, chunks[1], &mut app.template_list_state);

    // Help section with better styling - responsive
    let help_lines = if area.height >= min_height {
        vec![
            Line::from(vec![
                Span::styled("⌨️  Controls: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  ↑↓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw("/"),
                Span::styled("jk", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate  "),
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("/"),
                Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Select  "),
                Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw("/"),
                Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(": Exit"),
            ]),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("↑↓/jk", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::raw(": Navigate "),
                Span::styled("Enter/Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(": Select "),
                Span::styled("q/Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::raw(": Exit"),
            ]),
        ]
    };
    
    let help = Paragraph::new(help_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Gray))
                .title(" 💡 Help ")
                .title_style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
        );
    f.render_widget(help, chunks[2]);
}

fn draw_form_filling(f: &mut Frame, app: &mut App) {
    if let Some(template_idx) = app.selected_template {
        let (_, template) = &app.templates[template_idx];
        let area = f.area();
        let min_height = 18;
        let min_width = 70;
        
        // Responsive layout
        let (header_height, help_height) = if area.height < min_height {
            (3, 3)
        } else {
            (5, 4)
        };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(if area.width < min_width { 0 } else { 1 })
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(6),
                Constraint::Length(help_height),
            ].as_ref())
            .split(area);

        // Header with template info - responsive
        let header_lines = if area.height >= min_height {
            vec![
                Line::from(vec![
                    Span::styled("✏️ ", Style::default().fg(Color::Green)),
                    Span::styled("Form Filling", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("📝 ", Style::default().fg(Color::Yellow)),
                    Span::styled(&template.template.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(" • ", Style::default().fg(Color::Gray)),
                    Span::styled(&template.template.description, Style::default().fg(Color::Gray)),
                ]),
            ]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("✏️ ", Style::default().fg(Color::Green)),
                    Span::styled(&template.template.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]),
            ]
        };
        
        let header = Paragraph::new(header_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
                    .title(" 📋 Form Information ")
                    .title_style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            );
        f.render_widget(header, chunks[0]);

        // Form fields with better styling
        let field_names: Vec<_> = template.fields.keys().collect();
        let mut field_widgets = Vec::new();
        
        for (i, field_name) in field_names.iter().enumerate() {
            if let Some(field_config) = template.fields.get(*field_name) {
                let value = app.field_values.get(*field_name).cloned().unwrap_or_default();
                let is_current = i == app.current_field;
                let is_required = field_config.required.unwrap_or(false);
                
                let (icon, _style) = if is_current {
                    ("👉", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
                } else if !value.is_empty() {
                    ("✅", Style::default().fg(Color::Green))
                } else if is_required {
                    ("⚠️ ", Style::default().fg(Color::Red))
                } else {
                    ("📝", Style::default().fg(Color::Gray))
                };
                
                let display_value = if value.is_empty() && field_config.placeholder.is_some() {
                    field_config.placeholder.as_ref().unwrap().clone()
                } else if value.is_empty() {
                    "(empty)".to_string()
                } else {
                    value.clone()
                };
                
                field_widgets.push(ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(Color::Yellow)),
                    Span::styled(
                        format!("{}: ", field_config.label), 
                        if is_current {
                            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                        }
                    ),
                    Span::styled(
                        display_value.clone(),
                        if is_current {
                            Style::default().fg(Color::Black).bg(Color::Yellow)
                        } else if display_value == "(empty)" {
                            Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)
                        } else {
                            Style::default().fg(Color::Cyan)
                        }
                    ),
                ])));
            }
        }

        let fields = List::new(field_widgets)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title(" 📝 Form Fields ")
                    .title_style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            );

        f.render_widget(fields, chunks[1]);

        // Help section - responsive
        let help_lines = if area.height >= min_height {
            vec![
                Line::from(vec![
                    Span::styled("⌨️  Controls: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::styled("↑↓", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw("/"),
                    Span::styled("Tab", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(": Change field  "),
                    Span::styled("Type", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(": Edit  "),
                    Span::styled("Enter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(": Preview  "),
                    Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(": Back"),
                ]),
            ]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("↑↓/Tab", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(": Field "),
                    Span::styled("Type", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(": Edit "),
                    Span::styled("Enter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(": Preview"),
                ]),
            ]
        };
        
        let help = Paragraph::new(help_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray))
                    .title(" 💡 Help ")
                    .title_style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
            );
        f.render_widget(help, chunks[2]);
    }
}

fn draw_preview(f: &mut Frame, app: &mut App) {
    if let Some(template_idx) = app.selected_template {
        let (_, template) = &app.templates[template_idx];
        let area = f.area();
        let min_height = 16;
        
        // Responsive layout
        let (header_height, help_height) = if area.height < min_height {
            (3, 3)
        } else {
            (5, 4)
        };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(if area.width < 70 { 0 } else { 1 })
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(6),
                Constraint::Length(help_height),
            ].as_ref())
            .split(area);

        // Header - responsive
        let header_lines = if area.height >= min_height {
            vec![
                Line::from(vec![
                    Span::styled("👀 ", Style::default().fg(Color::Magenta)),
                    Span::styled("Preview", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("📤 ", Style::default().fg(Color::Yellow)),
                    Span::styled("Message to be sent to Discord:", Style::default().fg(Color::White)),
                ]),
            ]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("👀 ", Style::default().fg(Color::Magenta)),
                    Span::styled("Preview - Ready to send", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                ]),
            ]
        };
        
        let header = Paragraph::new(header_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta))
                    .title(" 🔍 Message Preview ")
                    .title_style(Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            );
        f.render_widget(header, chunks[0]);

        // Preview content with Discord-like styling
        let mut preview_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("📋 ", Style::default().fg(Color::Blue)),
                Span::styled("Embed Title: ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                Span::styled(&template.template.name, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("📄 ", Style::default().fg(Color::Gray)),
                Span::styled("Description: ", Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
                Span::styled(&template.template.description, Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("📝 ", Style::default().fg(Color::Yellow)),
                Span::styled("Form Data:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
        ];

        let mut field_count = 0;
        for (field_name, field_config) in &template.fields {
            if let Some(value) = app.field_values.get(field_name) {
                if !value.is_empty() {
                    field_count += 1;
                    preview_lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled("▸ ", Style::default().fg(Color::Green)),
                        Span::styled(format!("{}: ", field_config.label), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled(value.clone(), Style::default().fg(Color::White)),
                    ]));
                }
            }
        }

        if field_count == 0 {
            preview_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("⚠️ ", Style::default().fg(Color::Red)),
                Span::styled("No data entered yet", Style::default().fg(Color::Red).add_modifier(Modifier::ITALIC)),
            ]));
        }

        preview_lines.push(Line::from(""));
        
        // Bot info
        if let Some(username) = &template.webhook.username {
            preview_lines.push(Line::from(vec![
                Span::styled("🤖 ", Style::default().fg(Color::Blue)),
                Span::styled("Bot Name: ", Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
                Span::styled(username, Style::default().fg(Color::Cyan)),
            ]));
        }

        let preview = Paragraph::new(preview_lines)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue))
                    .title(" 💬 Discord Message ")
                    .title_style(Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)),
            );
        f.render_widget(preview, chunks[1]);

        // Action buttons - responsive
        let action_lines = if area.height >= min_height {
            vec![
                Line::from(vec![
                    Span::styled("🚀 ", Style::default().fg(Color::Green)),
                    Span::styled("Ready! ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw("Press "),
                    Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(" or "),
                    Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(" to send the message"),
                ]),
                Line::from(vec![
                    Span::styled("⌨️  ", Style::default().fg(Color::Cyan)),
                    Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(": Go back  "),
                    Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(": Exit"),
                ]),
            ]
        } else {
            vec![
                Line::from(vec![
                    Span::styled("Enter/Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(": Send "),
                    Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(": Back "),
                    Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::raw(": Exit"),
                ]),
            ]
        };
        
        let actions = Paragraph::new(action_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Gray))
                    .title(" 🎯 Actions ")
                    .title_style(Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)),
            );
        f.render_widget(actions, chunks[2]);
    }
}

fn draw_sending(f: &mut Frame) {
    let area = f.area();
    let popup_area = centered_rect(60, 25, area);
    
    f.render_widget(Clear, popup_area);
    
    let sending_content = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("        "),
            Span::styled("📡", Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Sending message...", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("Connecting to Discord servers", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled("Please wait...", Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
        ]),
        Line::from(""),
    ];
    
    let sending = Paragraph::new(sending_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" ⏳ Sending ")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        );
    f.render_widget(sending, popup_area);
}

fn draw_result(f: &mut Frame, success: bool, message: &str) {
    let area = f.area();
    let popup_area = centered_rect(70, 35, area);
    
    f.render_widget(Clear, popup_area);
    
    let (color, border_color, title, icon) = if success {
        (Color::Green, Color::Green, " ✅ Success! ", "🎉")
    } else {
        (Color::Red, Color::Red, " ❌ Error! ", "⚠️")
    };
    
    let mut result_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(icon, Style::default().fg(color)),
            Span::raw("  "),
            Span::styled(
                if success { "Operation completed successfully!" } else { "Operation failed!" },
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            ),
        ]),
        Line::from(""),
    ];

    // Message content with better formatting
    let lines: Vec<&str> = message.lines().collect();
    for line in lines {
        result_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(line, Style::default().fg(color)),
        ]));
    }

    result_lines.push(Line::from(""));
    result_lines.push(Line::from(""));
    
    // Action instructions
    result_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("⌨️  ", Style::default().fg(Color::Cyan)),
        Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(", "),
        Span::styled("Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(" or "),
        Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": Return to main menu"),
    ]));
    
    result_lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw(": Close application"),
    ]));

    let result = Paragraph::new(result_lines)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(title)
                .title_style(Style::default().fg(border_color).add_modifier(Modifier::BOLD)),
        );
    f.render_widget(result, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

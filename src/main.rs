mod config;
mod launcher;
mod theme;
mod ui;

use anyhow::{Context, Result};
use config::Settings;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, widgets::ListState};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const SETTINGS_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
enum SortMode {
    Name,
    LastPlayed,
    Playtime,
}

impl SortMode {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::LastPlayed,
            Self::LastPlayed => Self::Playtime,
            Self::Playtime => Self::Name,
        }
    }

    fn display(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::LastPlayed => "Last played",
            Self::Playtime => "Playtime",
        }
    }
}

#[derive(Debug, Clone)]
struct Instance {
    name: String,
    path: PathBuf,
    last_played: Option<String>,
    last_played_ts: Option<u64>,
    time_played: Option<String>,
    time_played_secs: Option<u64>,
    mc_version: Option<String>,
    mod_loader: Option<String>,
    mod_count: usize,
}

#[derive(Debug, Deserialize)]
struct InstanceConfig {
    #[serde(rename = "General")]
    general: GeneralConfig,
}

#[derive(Debug, Deserialize)]
struct GeneralConfig {
    name: String,
    #[serde(rename = "lastLaunchTime")]
    last_launch_time: Option<u64>,
    #[serde(rename = "totalTimePlayed")]
    total_time_played: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MMCPack {
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
struct Component {
    uid: String,
    version: Option<String>,
}

struct App {
    instances: Vec<Instance>,
    filtered_instances: Vec<Instance>,
    list_state: ListState,
    should_quit: bool,
    should_launch: bool,
    sort_mode: SortMode,
    search_mode: bool,
    search_query: String,
    details_mode: bool,
    settings: Settings,
    settings_open: bool,
    settings_selected: usize,
    help_open: bool,
    status: Option<String>,
    running_instances: HashSet<PathBuf>,
    demo: bool,
}

impl App {
    fn new() -> Result<Self> {
        let settings = Settings::load()?;
        let mut app = Self::from_instances(load_instances()?, settings);
        app.refresh_running();
        Ok(app)
    }

    fn from_instances(instances: Vec<Instance>, settings: Settings) -> Self {
        let mut app = Self {
            instances,
            filtered_instances: Vec::new(),
            list_state: ListState::default(),
            should_quit: false,
            should_launch: false,
            sort_mode: SortMode::Name,
            search_mode: false,
            search_query: String::new(),
            details_mode: settings.details,
            settings,
            settings_open: false,
            settings_selected: 0,
            help_open: false,
            status: None,
            running_instances: HashSet::new(),
            demo: false,
        };
        app.sort_instances();
        app.update_filter();
        app
    }

    fn demo() -> Self {
        let instances = [
            (
                "A quiet world",
                "1.21.5",
                "Fabric",
                37,
                146_400,
                "2 hours ago",
            ),
            (
                "Better adventures",
                "1.20.1",
                "Forge",
                124,
                27_000,
                "Yesterday",
            ),
            (
                "Create something",
                "1.20.1",
                "NeoForge",
                86,
                68_520,
                "3 days ago",
            ),
            ("The original", "1.12.2", "Vanilla", 0, 7_440, "Last week"),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(i, (name, version, loader, mods, seconds, last))| Instance {
                name: name.into(),
                path: PathBuf::from(format!("/demo/PrismLauncher/instances/world-{}", i + 1)),
                last_played: Some(last.into()),
                last_played_ts: Some(4 - i as u64),
                time_played: Some(format_duration(seconds)),
                time_played_secs: Some(seconds),
                mc_version: Some(version.into()),
                mod_loader: Some(loader.into()),
                mod_count: mods,
            },
        )
        .collect();
        let mut app = Self::from_instances(instances, Settings::default());
        app.demo = true;
        app
    }

    fn selected(&self) -> Option<&Instance> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered_instances.get(i))
    }

    fn next(&mut self) {
        if !self.filtered_instances.is_empty() {
            let i = self
                .list_state
                .selected()
                .map_or(0, |i| (i + 1) % self.filtered_instances.len());
            self.list_state.select(Some(i));
        }
    }

    fn previous(&mut self) {
        if !self.filtered_instances.is_empty() {
            let len = self.filtered_instances.len();
            let i = self
                .list_state
                .selected()
                .map_or(0, |i| (i + len - 1) % len);
            self.list_state.select(Some(i));
        }
    }

    fn request_launch(&mut self) {
        if let Some(instance) = self.selected() {
            if self.demo {
                self.status = Some("Preview only · no game will be launched".into());
            } else if self.running_instances.contains(&instance.path) {
                self.status = Some("This instance is already running".into());
            } else {
                // Preserve the filtered selection until after terminal restoration.
                self.should_launch = true;
                self.should_quit = true;
            }
        }
    }

    fn launch_selected(&self) -> Result<()> {
        if let Some(instance) = self.selected() {
            launcher::launch_instance(&instance.path, self.settings.hide_launcher)?;
        }
        Ok(())
    }

    fn open_folder_selected(&self) -> Result<()> {
        if let Some(instance) = self.selected()
            && !self.demo
        {
            Command::new("xdg-open")
                .arg(&instance.path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("Could not open the instance folder")?;
        }
        Ok(())
    }

    fn sort_instances(&mut self) {
        self.instances.sort_by(|a, b| match self.sort_mode {
            SortMode::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortMode::LastPlayed => b
                .last_played_ts
                .cmp(&a.last_played_ts)
                .then_with(|| a.name.cmp(&b.name)),
            SortMode::Playtime => b
                .time_played_secs
                .cmp(&a.time_played_secs)
                .then_with(|| a.name.cmp(&b.name)),
        });
    }

    fn update_filter(&mut self) {
        let selected_path = self.selected().map(|instance| instance.path.clone());
        let query = self.search_query.to_lowercase();
        self.filtered_instances = self
            .instances
            .iter()
            .filter(|instance| instance.name.to_lowercase().contains(&query))
            .cloned()
            .collect();
        let selection = selected_path
            .and_then(|path| self.filtered_instances.iter().position(|i| i.path == path))
            .or_else(|| (!self.filtered_instances.is_empty()).then_some(0));
        self.list_state.select(selection);
    }

    fn refresh(&mut self) -> Result<()> {
        if !self.demo {
            self.instances = load_instances()?;
            self.sort_instances();
            self.update_filter();
            self.refresh_running();
        }
        self.status = Some("Library refreshed".into());
        Ok(())
    }

    fn refresh_running(&mut self) {
        if !self.demo {
            self.running_instances = running_instances(&self.instances);
        }
    }

    fn save_settings(&mut self) {
        if !self.demo {
            self.status = match self.settings.save() {
                Ok(()) => Some("Settings saved".into()),
                Err(err) => Some(format!("Could not save settings: {err:#}")),
            };
        }
    }

    fn change_setting(&mut self, forward: bool) {
        match self.settings_selected {
            0 => self.settings.hide_launcher = !self.settings.hide_launcher,
            1 => {
                self.settings.details = !self.settings.details;
                self.details_mode = self.settings.details;
            }
            2 => {
                self.settings.theme = if forward {
                    self.settings.theme.next()
                } else {
                    self.settings.theme.previous()
                };
            }
            _ => return,
        }
        self.save_settings();
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }
        if self.settings_open {
            match key.code {
                KeyCode::Esc | KeyCode::Char(',') | KeyCode::Char('q') => {
                    self.settings_open = false
                }
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                    self.settings_selected = (self.settings_selected + 1) % SETTINGS_COUNT;
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                    self.settings_selected =
                        (self.settings_selected + SETTINGS_COUNT - 1) % SETTINGS_COUNT;
                }
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => self.change_setting(true),
                KeyCode::Left => self.change_setting(false),
                _ => {}
            }
        } else if self.help_open {
            self.help_open = false;
        } else if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_query.clear();
                    self.update_filter();
                }
                KeyCode::Enter => self.request_launch(),
                KeyCode::Down => self.next(),
                KeyCode::Up => self.previous(),
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_filter();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.search_query.push(c);
                    self.update_filter();
                }
                _ => {}
            }
        } else {
            self.status = None;
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                KeyCode::Down | KeyCode::Char('j') => self.next(),
                KeyCode::Up | KeyCode::Char('k') => self.previous(),
                KeyCode::Enter => self.request_launch(),
                KeyCode::Char('o') => self.open_folder_selected()?,
                KeyCode::Char('s') => {
                    self.sort_mode = self.sort_mode.next();
                    self.sort_instances();
                    self.update_filter();
                }
                KeyCode::Char('/') => self.search_mode = true,
                KeyCode::Char('i') => {
                    self.details_mode = !self.details_mode;
                    self.settings.details = self.details_mode;
                    self.save_settings();
                }
                KeyCode::Char('r') => self.refresh()?,
                KeyCode::Char(',') => self.settings_open = true,
                KeyCode::Char('?') => self.help_open = true,
                _ => {}
            }
        }
        Ok(())
    }
}

fn instance_roots() -> Result<Vec<PathBuf>> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    Ok(vec![
        data_home.join("PrismLauncher/instances"),
        home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher/instances"),
    ])
}

fn load_instances() -> Result<Vec<Instance>> {
    load_instances_from(&instance_roots()?)
}

fn load_instances_from(roots: &[PathBuf]) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    let mut seen = HashSet::new();
    for root in roots.iter().filter(|p| p.is_dir()) {
        for entry in
            fs::read_dir(root).with_context(|| format!("Could not read {}", root.display()))?
        {
            let path = entry?.path();
            if !path.is_dir()
                || !seen.insert(fs::canonicalize(&path).unwrap_or_else(|_| path.clone()))
            {
                continue;
            }
            let Ok(content) = fs::read_to_string(path.join("instance.cfg")) else {
                continue;
            };
            let Ok(config) = serde_ini::from_str::<InstanceConfig>(&content) else {
                continue;
            };
            let pack = fs::read_to_string(path.join("mmc-pack.json"))
                .ok()
                .and_then(|s| serde_json::from_str::<MMCPack>(&s).ok());
            let mc_version = pack.as_ref().and_then(|pack| {
                pack.components
                    .iter()
                    .find(|c| c.uid == "net.minecraft")
                    .and_then(|c| c.version.clone())
            });
            let mod_loader = pack
                .as_ref()
                .map(|pack| detect_loader(&pack.components).to_string());
            let mods_dir = path.join(".minecraft/mods");
            let mod_count = fs::read_dir(mods_dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| {
                            e.path()
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
                        })
                        .count()
                })
                .unwrap_or(0);
            instances.push(Instance {
                name: config.general.name,
                path,
                last_played: config
                    .general
                    .last_launch_time
                    .filter(|&ts| ts > 0)
                    .map(format_timestamp),
                last_played_ts: config.general.last_launch_time.filter(|&ts| ts > 0),
                time_played: config.general.total_time_played.map(format_duration),
                time_played_secs: config.general.total_time_played,
                mc_version,
                mod_loader,
                mod_count,
            });
        }
    }
    Ok(instances)
}

fn detect_loader(components: &[Component]) -> &'static str {
    for (uid, label) in [
        ("net.neoforged", "NeoForge"),
        ("org.quiltmc", "Quilt"),
        ("net.fabricmc", "Fabric"),
        ("net.minecraftforge", "Forge"),
    ] {
        if components.iter().any(|c| c.uid.starts_with(uid)) {
            return label;
        }
    }
    "Vanilla"
}

fn running_instances(instances: &[Instance]) -> HashSet<PathBuf> {
    let mut running = HashSet::new();
    let Ok(processes) = fs::read_dir("/proc") else {
        return running;
    };
    for entry in processes.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let proc_path = entry.path();
        let Ok(exe) = fs::read_link(proc_path.join("exe")) else {
            continue;
        };
        if !exe.file_name().is_some_and(|n| n == "java" || n == "javaw") {
            continue;
        }
        let args = fs::read(proc_path.join("cmdline")).unwrap_or_default();
        let cwd = fs::read_link(proc_path.join("cwd")).ok();
        for instance in instances {
            if cwd.as_ref().is_some_and(|p| p.starts_with(&instance.path))
                || args.split(|&b| b == 0).any(|arg| {
                    Path::new(std::ffi::OsStr::from_bytes(arg)).starts_with(&instance.path)
                })
            {
                running.insert(instance.path.clone());
            }
        }
    }
    running
}

use std::os::unix::ffi::OsStrExt;

fn format_timestamp(timestamp_ms: u64) -> String {
    let elapsed = UNIX_EPOCH
        .checked_add(Duration::from_millis(timestamp_ms))
        .and_then(|time| SystemTime::now().duration_since(time).ok());
    match elapsed.map(|d| d.as_secs()) {
        Some(seconds) if seconds >= 86_400 => format!("{}d ago", seconds / 86_400),
        Some(seconds) if seconds >= 3_600 => format!("{}h ago", seconds / 3_600),
        Some(seconds) if seconds >= 60 => format!("{}m ago", seconds / 60),
        Some(_) => "Just now".into(),
        None => "Recently".into(),
    }
}

fn format_duration(seconds: u64) -> String {
    match (seconds / 3600, (seconds % 3600) / 60) {
        (hours, minutes) if hours > 0 => format!("{hours}h {minutes}m"),
        (_, minutes) if minutes > 0 => format!("{minutes}m"),
        _ => format!("{seconds}s"),
    }
}

fn main() -> Result<()> {
    let mut demo = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--demo" => demo = true,
            "--help" | "-h" => {
                println!(
                    "minecraft-tui\n\nUsage: minecraft-tui [--demo]\n\n  --demo  Preview with sample instances (no games or settings are changed)\n  --help  Show this help\n\nPress , for settings or ? for keyboard shortcuts."
                );
                return Ok(());
            }
            _ => anyhow::bail!("Unknown option: {arg}. Use --help for usage."),
        }
    }
    // Load fallible configuration before entering the terminal's alternate screen.
    let mut app = if demo { App::demo() } else { App::new()? };
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        previous_hook(info);
    }));
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal, &mut app);
    let restored = restore_terminal(&mut terminal);
    result?;
    restored?;
    if app.should_launch {
        app.launch_selected()?;
    }
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(err.into());
    }
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(err) => {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            Err(err.into())
        }
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let raw_result = disable_raw_mode();
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    raw_result?;
    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut last_running_check = Instant::now();
    while !app.should_quit {
        terminal.draw(|frame| ui::ui(frame, app))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && let Err(err) = app.handle_key(key)
        {
            app.status = Some(format!("{err:#}"));
        }
        if last_running_check.elapsed() >= Duration::from_secs(2) {
            app.refresh_running();
            last_running_check = Instant::now();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn filtered_enter_launches_the_displayed_instance() {
        let mut app = App::demo();
        app.demo = false;
        app.handle_key(key(KeyCode::Char('/'))).unwrap();
        for c in "original".chars() {
            app.handle_key(key(KeyCode::Char(c))).unwrap();
        }
        assert_eq!(app.filtered_instances.len(), 1);
        app.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(app.should_launch);
        assert_eq!(app.selected().unwrap().name, "The original");
    }

    #[test]
    fn empty_search_never_requests_a_launch_or_invalid_selection() {
        let mut app = App::demo();
        app.demo = false;
        app.search_mode = true;
        app.search_query = "no such instance".into();
        app.update_filter();
        app.handle_key(key(KeyCode::Enter)).unwrap();
        app.next();
        app.previous();
        assert!(!app.should_quit);
        assert!(!app.should_launch);
        assert_eq!(app.list_state.selected(), None);
    }

    #[test]
    fn sort_and_clear_filter_preserve_selection_by_path() {
        let mut app = App::demo();
        app.next();
        let selected = app.selected().unwrap().path.clone();
        app.handle_key(key(KeyCode::Char('s'))).unwrap();
        assert_eq!(app.selected().unwrap().path, selected);
        app.search_mode = true;
        app.search_query = "Better".into();
        app.update_filter();
        app.handle_key(key(KeyCode::Esc)).unwrap();
        assert!(!app.search_mode);
        assert_eq!(app.filtered_instances.len(), 4);
        assert_eq!(app.selected().unwrap().path, selected);
    }

    #[test]
    fn settings_capture_keys_and_demo_never_launches() {
        let mut app = App::demo();
        app.handle_key(key(KeyCode::Char(','))).unwrap();
        app.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(app.settings.hide_launcher);
        assert!(!app.should_launch);
        app.handle_key(key(KeyCode::Esc)).unwrap();
        app.handle_key(key(KeyCode::Enter)).unwrap();
        assert!(!app.should_launch);
        assert!(!app.should_quit);
        assert!(app.status.unwrap().contains("Preview"));
    }

    #[test]
    fn themes_change_in_both_directions_without_toggling_other_preferences() {
        use theme::Theme;

        let mut app = App::demo();
        assert_eq!(app.settings.theme, Theme::Terminal);
        app.handle_key(key(KeyCode::Char(','))).unwrap();
        // Up wraps from the first setting to the theme picker.
        app.handle_key(key(KeyCode::Up)).unwrap();
        assert_eq!(app.settings_selected, 2);
        let hide_launcher = app.settings.hide_launcher;
        let details = app.settings.details;
        app.handle_key(key(KeyCode::Right)).unwrap();
        assert_eq!(app.settings.theme, Theme::Sage);
        app.handle_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.settings.theme, Theme::Violet);
        app.handle_key(key(KeyCode::Left)).unwrap();
        assert_eq!(app.settings.theme, Theme::Sage);
        assert_eq!(app.settings.hide_launcher, hide_launcher);
        assert_eq!(app.settings.details, details);
        assert!(!app.should_launch);
        app.handle_key(key(KeyCode::Tab)).unwrap();
        assert_eq!(app.settings_selected, 0);
        app.handle_key(key(KeyCode::BackTab)).unwrap();
        assert_eq!(app.settings_selected, 2);
    }

    #[test]
    fn running_instance_cannot_be_launched_twice() {
        let mut app = App::demo();
        app.demo = false;
        app.running_instances
            .insert(app.selected().unwrap().path.clone());
        app.request_launch();
        assert!(!app.should_launch);
        assert!(!app.should_quit);
    }

    #[test]
    fn loads_prism_metadata_and_mods_without_confusing_neoforge() {
        let root =
            std::env::temp_dir().join(format!("minecraft-tui-metadata-{}", std::process::id()));
        let instance = root.join("folder-id");
        fs::create_dir_all(instance.join(".minecraft/mods")).unwrap();
        fs::write(
            instance.join("instance.cfg"),
            "[General]\nname=Different display name\nlastLaunchTime=0\ntotalTimePlayed=3661\n",
        )
        .unwrap();
        fs::write(instance.join("mmc-pack.json"), r#"{"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.neoforged","version":"21.1"}]}"#).unwrap();
        fs::write(instance.join(".minecraft/mods/mod.jar"), "").unwrap();
        fs::write(instance.join(".minecraft/mods/disabled.jar.disabled"), "").unwrap();
        let result = load_instances_from(&[root.clone(), root.clone()]).unwrap();
        fs::remove_dir_all(root).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Different display name");
        assert_eq!(result[0].mod_loader.as_deref(), Some("NeoForge"));
        assert_eq!(result[0].mc_version.as_deref(), Some("1.21.1"));
        assert_eq!(result[0].mod_count, 1);
        assert_eq!(result[0].last_played, None);
        assert_eq!(result[0].time_played.as_deref(), Some("1h 1m"));
    }

    #[test]
    fn formats_playtime_and_cycles_sort() {
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(120), "2m");
        assert_eq!(format_duration(45), "45s");
        assert_eq!(SortMode::Name.next().next().next(), SortMode::Name);
    }
}

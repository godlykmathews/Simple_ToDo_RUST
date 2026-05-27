use serde::{Deserialize, Serialize};
use slint::{ModelRc, SharedString, VecModel, Weak};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use thiserror::Error;

slint::include_modules!();

type Result<T> = std::result::Result<T, TodoError>;

#[derive(Debug, Error)]
enum TodoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("could not determine XDG data directory; set XDG_DATA_HOME or HOME")]
    MissingDataDir,

    #[error("task with id {0} was not found")]
    TaskNotFound(i32),

    #[error("application state is unavailable")]
    StateUnavailable,

    #[error("UI error: {0}")]
    Ui(#[from] slint::PlatformError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Task {
    id: i32,
    title: String,
    completed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct TaskStore {
    tasks: Vec<Task>,
    next_id: i32,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }
}

impl TaskStore {
    fn normalize_next_id(&mut self) {
        if self.next_id > 0 {
            return;
        }

        let max_id = self.tasks.iter().map(|task| task.id).max();
        self.next_id = match max_id {
            Some(id) => id.saturating_add(1),
            None => 1,
        };
    }

    fn add(&mut self, title: String) {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        if self.next_id <= 0 {
            self.next_id = i32::MAX;
        }

        self.tasks.push(Task {
            id,
            title,
            completed: false,
        });
    }

    fn set_completed(&mut self, id: i32, completed: bool) -> Result<()> {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) else {
            return Err(TodoError::TaskNotFound(id));
        };

        task.completed = completed;
        Ok(())
    }

    fn delete(&mut self, id: i32) -> Result<()> {
        let original_len = self.tasks.len();
        self.tasks.retain(|task| task.id != id);

        if self.tasks.len() == original_len {
            return Err(TodoError::TaskNotFound(id));
        }

        Ok(())
    }
}

struct JsonStorage {
    path: PathBuf,
}

impl JsonStorage {
    fn new() -> Result<Self> {
        Ok(Self {
            path: xdg_data_dir()?.join("vibe_todo_gui").join("tasks.json"),
        })
    }

    fn load(&self) -> Result<TaskStore> {
        if !self.path.exists() {
            return Ok(TaskStore::default());
        }

        let mut file = File::open(&self.path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        if contents.trim().is_empty() {
            return Ok(TaskStore::default());
        }

        let mut store: TaskStore = serde_json::from_str(&contents)?;
        store.normalize_next_id();
        Ok(store)
    }

    fn save(&self, store: &TaskStore) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let temp_path = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(store)?;

        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temp_path)?;

            file.write_all(json.as_bytes())?;
            file.write_all(b"\n")?;
            file.flush()?;
            file.sync_all()?;
        }

        fs::rename(&temp_path, &self.path)?;

        if let Some(parent) = self.path.parent() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }

        Ok(())
    }
}

struct AppState {
    store: TaskStore,
    storage: JsonStorage,
}

impl AppState {
    fn load() -> Result<Self> {
        let storage = JsonStorage::new()?;
        let store = storage.load()?;
        Ok(Self { store, storage })
    }

    fn save(&self) -> Result<()> {
        self.storage.save(&self.store)
    }
}

fn xdg_data_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(path));
    }

    if let Some(home) = env::var_os("HOME") {
        return Ok(Path::new(&home).join(".local").join("share"));
    }

    Err(TodoError::MissingDataDir)
}

fn lock_state(state: &Arc<Mutex<AppState>>) -> Result<MutexGuard<'_, AppState>> {
    state.lock().map_err(|_| TodoError::StateUnavailable)
}

fn todo_items(store: &TaskStore) -> ModelRc<TodoItem> {
    let items: Vec<TodoItem> = store
        .tasks
        .iter()
        .map(|task| TodoItem {
            id: task.id,
            title: SharedString::from(task.title.as_str()),
            completed: task.completed,
        })
        .collect();

    ModelRc::new(VecModel::from(items))
}

fn set_error(ui: &AppWindow, error: TodoError) {
    ui.set_status_message(SharedString::from(format!("{error}")));
}

fn refresh_tasks(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Result<()> {
    let state = lock_state(state)?;
    ui.set_tasks(todo_items(&state.store));
    Ok(())
}

fn save_and_refresh(ui: &AppWindow, state: &Arc<Mutex<AppState>>) -> Result<()> {
    {
        let state = lock_state(state)?;
        state.save()?;
    }

    refresh_tasks(ui, state)?;
    ui.set_status_message(SharedString::from(""));
    Ok(())
}

fn with_window(window: &Weak<AppWindow>, action: impl FnOnce(AppWindow)) {
    if let Some(ui) = window.upgrade() {
        action(ui);
    }
}

fn connect_callbacks(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let weak = ui.as_weak();
    let add_state = Arc::clone(&state);
    ui.on_add_task(move |title| {
        let trimmed = title.trim().to_string();
        with_window(&weak, |ui| {
            if trimmed.is_empty() {
                ui.set_status_message(SharedString::from("Type a task before adding it."));
                return;
            }

            let result = lock_state(&add_state).map(|mut state| {
                state.store.add(trimmed);
            });

            match result.and_then(|()| save_and_refresh(&ui, &add_state)) {
                Ok(()) => ui.set_new_task_text(SharedString::from("")),
                Err(error) => set_error(&ui, error),
            }
        });
    });

    let weak = ui.as_weak();
    let completed_state = Arc::clone(&state);
    ui.on_set_completed(move |id, completed| {
        with_window(&weak, |ui| {
            let result = lock_state(&completed_state)
                .and_then(|mut state| state.store.set_completed(id, completed));

            if let Err(error) = result.and_then(|()| save_and_refresh(&ui, &completed_state)) {
                set_error(&ui, error);
            }
        });
    });

    let weak = ui.as_weak();
    let delete_state = Arc::clone(&state);
    ui.on_delete_task(move |id| {
        with_window(&weak, |ui| {
            let result = lock_state(&delete_state).and_then(|mut state| state.store.delete(id));

            if let Err(error) = result.and_then(|()| save_and_refresh(&ui, &delete_state)) {
                set_error(&ui, error);
            }
        });
    });
}

fn run() -> Result<()> {
    let ui = AppWindow::new()?;

    let (state, startup_error) = match AppState::load() {
        Ok(state) => (state, None),
        Err(error) => {
            let fallback_storage = JsonStorage::new()?;
            (
                AppState {
                    store: TaskStore::default(),
                    storage: fallback_storage,
                },
                Some(error),
            )
        }
    };

    let state = Arc::new(Mutex::new(state));
    refresh_tasks(&ui, &state)?;

    if let Some(error) = startup_error {
        set_error(&ui, error);
    }

    connect_callbacks(&ui, state);
    ui.run()?;

    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
    }
}

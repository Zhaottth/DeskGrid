use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};

use crate::layout::{RECYCLE, full_path, namespace_id, path_key};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub key: String,
    pub name: String,
    pub label: String,
    pub extension: String,
    pub folder: bool,
    pub modified: Option<SystemTime>,
    pub bytes: Option<u64>,
}

impl Entry {
    fn from_metadata(path: PathBuf, info: Metadata) -> Self {
        #[cfg(windows)]
        let folder = {
            use std::os::windows::fs::MetadataExt;
            info.file_attributes() & 0x10 != 0
        };
        #[cfg(not(windows))]
        let folder = info.is_dir();
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .into_owned();
        let extension = if folder {
            String::new()
        } else {
            path.extension()
                .map_or_else(String::new, |ext| ext.to_string_lossy().to_lowercase())
        };
        let label = if matches!(extension.as_str(), "lnk" | "url") {
            path.file_stem()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned()
        } else {
            name.clone()
        };
        Self {
            key: path_key(&path),
            path,
            name,
            label,
            extension,
            folder,
            modified: info.modified().ok(),
            bytes: (!folder).then_some(info.len()),
        }
    }

    pub fn recycle() -> Self {
        Self {
            path: RECYCLE.into(),
            key: RECYCLE.into(),
            name: tr!("回收站", "資源回收筒", "Recycle Bin").into(),
            label: tr!("回收站", "資源回收筒", "Recycle Bin").into(),
            extension: String::new(),
            folder: false,
            modified: None,
            bytes: None,
        }
    }

    pub fn is_recycle(&self) -> bool {
        self.key == RECYCLE
    }

    pub fn namespace(path: PathBuf, label: String) -> Self {
        Self {
            key: path_key(&path),
            name: label.clone(),
            label,
            path,
            extension: String::new(),
            folder: false,
            modified: None,
            bytes: None,
        }
    }

    pub fn is_namespace(&self) -> bool {
        namespace_id(&self.path).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Available(Vec<Entry>),
    Unavailable,
}

impl Listing {
    pub fn entries(&self) -> &[Entry] {
        if let Self::Available(entries) = self {
            entries
        } else {
            &[]
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Source {
    key: String,
    file: bool,
}

struct Feed {
    path: PathBuf,
    pending: Option<u64>,
    next: Instant,
    listing: Listing,
}

struct Job {
    token: u64,
    source: Source,
    path: PathBuf,
}

struct Result {
    token: u64,
    source: Source,
    listing: Listing,
}

pub struct Scanner {
    feeds: HashMap<Source, Feed>,
    active: HashSet<Source>,
    desktop_jobs: mpsc::SyncSender<Job>,
    jobs: mpsc::SyncSender<Job>,
    results: mpsc::Receiver<Result>,
    token: u64,
}

impl Scanner {
    pub fn new() -> std::io::Result<Self> {
        let (sender, results) = mpsc::channel();
        Ok(Self {
            feeds: HashMap::new(),
            active: HashSet::new(),
            desktop_jobs: workers(2, &sender)?,
            jobs: workers(4, &sender)?,
            results,
            token: 0,
        })
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.results.try_recv() {
            let Some(feed) = self.feeds.get_mut(&result.source) else {
                continue;
            };
            if feed.pending != Some(result.token) {
                continue;
            }
            feed.pending = None;
            feed.next = Instant::now() + Duration::from_secs(2);
            if feed.listing != result.listing {
                feed.listing = result.listing;
                changed = true;
            }
        }
        changed
    }

    pub fn add(&mut self, path: &Path, file: bool, desktop: bool) -> Source {
        let source = Source {
            key: path_key(path),
            file,
        };
        self.active.insert(source.clone());
        let feed = self.feeds.entry(source.clone()).or_insert_with(|| Feed {
            path: path.to_owned(),
            pending: None,
            next: Instant::now(),
            listing: Listing::Loading,
        });
        if feed.pending.is_none() && Instant::now() >= feed.next {
            self.token = self.token.wrapping_add(1);
            let job = Job {
                token: self.token,
                source: source.clone(),
                path: feed.path.clone(),
            };
            let queue = if desktop {
                &self.desktop_jobs
            } else {
                &self.jobs
            };
            if queue.try_send(job).is_ok() {
                feed.pending = Some(self.token);
            }
        }
        source
    }

    pub fn prune(&mut self) {
        self.feeds.retain(|source, _| self.active.contains(source));
        self.active.clear();
    }

    pub fn refresh(&mut self) {
        for feed in self.feeds.values_mut() {
            feed.next = Instant::now();
        }
    }

    pub fn get(&self, source: &Source) -> &Listing {
        self.feeds
            .get(source)
            .map_or(&Listing::Loading, |feed| &feed.listing)
    }
}

fn workers(count: usize, results: &mpsc::Sender<Result>) -> std::io::Result<mpsc::SyncSender<Job>> {
    let (sender, jobs) = mpsc::sync_channel::<Job>(128);
    let jobs = Arc::new(Mutex::new(jobs));
    for _ in 0..count {
        let jobs = jobs.clone();
        let results = results.clone();
        std::thread::Builder::new()
            .name("DeskGrid directory scan".into())
            .spawn(move || {
                loop {
                    let job = jobs.lock().unwrap().recv();
                    let Ok(job) = job else { break };
                    let listing = scan(&job.path, job.source.file);
                    if results
                        .send(Result {
                            source: job.source,
                            token: job.token,
                            listing,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
    }
    Ok(sender)
}

fn scan(path: &Path, file: bool) -> Listing {
    if file {
        return fs::symlink_metadata(path).map_or(Listing::Unavailable, |info| {
            Listing::Available(vec![Entry::from_metadata(
                full_path(path).unwrap_or_else(|| path.to_owned()),
                info,
            )])
        });
    }
    let absolute = full_path(path);
    let path = absolute.as_deref().unwrap_or(path);
    let Ok(directory) = fs::read_dir(path) else {
        return Listing::Unavailable;
    };
    let mut entries = Vec::new();
    for item in directory {
        let Ok(item) = item else {
            return Listing::Unavailable;
        };
        let Ok(info) = item.metadata() else { continue };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if info.file_attributes() & (0x02 | 0x04) != 0 {
                continue;
            }
        }
        entries.push(Entry::from_metadata(item.path(), info));
    }
    entries.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    Listing::Available(entries)
}

use std::sync::{LockResult,MutexGuard,Arc,Mutex};
use std::path::PathBuf;
use derivative::*;

use chomper_plugin::*;

use dynamic_reload::{DynamicReload, PlatformName, Search, UpdateState};
use std::time::Duration;

#[derive(Debug)]
pub enum CreatePluginError {
    CouldNotCreateShadowDir {
        error: std::io::Error,
    },
    NoEntryPoint {
        error: dynamic_reload::libloading::Error,
    },
    AddLibraryError {
        error: dynamic_reload::Error,
    },
    BadFileString,

    #[allow(dead_code)]
    MutexError {
        //describes which mutex was poisoned
        tag:  &'static str,
    },

    NoDylibParentDir,
    EnvNotSet,

    #[allow(dead_code)]
    NoChecksum,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct KlebsPluginInterface {
    lib:            Arc<Mutex<Option<Arc<dynamic_reload::Lib>>>>,
    plugin:         Arc<Mutex<Option<Box<dyn KlebsFixBabyRustPlugin>>>>,
    checksum:       Arc<Mutex<Option<String>>>,
    reload_handler: Arc<Mutex<DynamicReload>>,
}

impl KlebsPluginInterface {

    #[tracing::instrument]
    pub fn new() -> Result<Self, CreatePluginError> {

        tracing::debug!("KlebsPluginInterface::new()");

        let mut reload_handler = Self::create_dynamic_reload_handler()?;

        tracing::debug!("reload_handler: {:?}", &reload_handler);

        let lib = unsafe { 
            reload_handler.add_library("chomper2", PlatformName::Yes).map_err(|error| {
                CreatePluginError::AddLibraryError {
                    error,
                }
            })?
        };

        tracing::debug!("lib: {:?}", &lib);

        let plugin = Self::create_plugin_from_library_entrypoint(&lib)?;

        tracing::debug!("plugin: {:?}", &plugin);

        let checksum = Self::compute_dylib_checksum()?;

        tracing::debug!("checksum: {:?}", &checksum);

        Ok(
            Self {
                lib:            Arc::new(Mutex::new(Some(lib))),
                plugin:         Arc::new(Mutex::new(Some(plugin))),
                checksum:       Arc::new(Mutex::new(Some(checksum))),
                reload_handler: Arc::new(Mutex::new(reload_handler)),
            }
        )
    }

    #[tracing::instrument]
    pub fn plugin<'a>(&'a self) -> LockResult<MutexGuard<'a, Option<Box<dyn KlebsFixBabyRustPlugin>>>>  {
        self.plugin.lock()
    }

    #[tracing::instrument]
    pub fn maybe_reload_plugin(&mut self) {

        tracing::debug!("maybe_reload_plugin");

        //is this clone acceptable?
        let reload_handler = self.reload_handler.clone();

        match reload_handler.lock() {
            Ok(mut guard) => {
                unsafe {
                    guard.update(&KlebsPluginInterface::reload_callback, self);
                }
            },
            Err(e) => {
                tracing::error!("could not lock reload_handler mutex! error! {:?}", e);
            }
        };
    }
}

//--------------------------------[ keep private ]
impl KlebsPluginInterface {

    #[tracing::instrument]
    fn set_lib(&mut self, lib: Option<&Arc<dynamic_reload::Lib>>) {
        match self.lib.lock() {
            Ok(mut guard) => *guard = lib.cloned(),
            Err(e)    => tracing::error!("could not unlock lib! error! {:?}", e),
        }
    }

    #[tracing::instrument]
    fn set_plugin(&mut self, plugin: Option<Box<dyn KlebsFixBabyRustPlugin>>) {
        match self.plugin.lock() {
            Ok(mut guard) => *guard = plugin,
            Err(e)    => tracing::error!("could not unlock plugin! error! {:?}", e),
        }
    }

    #[tracing::instrument]
    fn set_checksum(&mut self, checksum: Option<String>) {
        match self.checksum.lock() {
            Ok(mut guard) => *guard = checksum,
            Err(e)    => tracing::error!("could not unlock checksum! error! {:?}", e),
        }
    }

    #[tracing::instrument]
    fn unload_lib(&mut self) {
        //self.set_lib(None);
        //self.set_plugin(None);
        //self.set_checksum(None);
    }

    #[tracing::instrument]
    fn old_checksum(&self) -> String {
        tracing::info!("about to calculate old_checksum");
        self.checksum.lock().unwrap().clone().unwrap()
    }

    // can fail if we fail plugin creation
    #[tracing::instrument]
    fn maybe_reload_lib(&mut self, lib: &Arc<dynamic_reload::Lib>) {

        let old_checksum = self.old_checksum();

        tracing::info!("old_checksum: {:?}", old_checksum);

        match Self::create_plugin_from_library_entrypoint(&lib) {
            Ok(plugin) => {

                match Self::compute_dylib_checksum() {
                    Ok(new_checksum) => {

                        match new_checksum == old_checksum {
                            true => {
                                tracing::info!(
                                    "new_checksum {:?} equals old {:?}", 
                                    new_checksum,
                                    old_checksum
                                );
                            },
                            false => {
                                self.set_lib(Some(lib));
                                self.set_plugin(Some(plugin));
                                self.set_checksum(Some(new_checksum));
                            }
                        }
                    },
                    Err(e) => {
                        tracing::error!("could not compute checksum! error! {:?}", e);
                    }
                }
            },
            Err(e) => {

                tracing::error!("could not create_plugin_from_library_entrypoint! error! {:?}", e);
            }
        }
    }

    // called when a lib needs to be reloaded.
    //
    // the UpdateState enum determines the
    // callsite, and the ordering of operations
    // within the caller.
    //
    // as far as I can tell, it is used to avoid
    // writing several callbacks
    #[tracing::instrument]
    fn reload_callback(&mut self, 
        state: UpdateState, 
        lib:   Option<&Arc<dynamic_reload::Lib>>)
    {
        match state {
            UpdateState::Before          => Self::unload_lib(self),

            // here, lib is known to be Some
            // because UpdateState::After, the
            // only caller guarantees this
            UpdateState::After           => Self::maybe_reload_lib(self, lib.unwrap()),

            UpdateState::ReloadFailed(_) => println!("Failed to reload"),
        }
    }

    #[tracing::instrument]
    fn compute_dylib_checksum() -> Result<String,CreatePluginError> {

        tracing::debug!("compute_dylib_checksum");

        let path = Self::default_plugin_dylib_path()?;

        tracing::debug!("compute_dylib_checksum, dylib_path: {:?}", path);

        Ok(checksums::hash_file(path.as_path(), checksums::Algorithm::CRC64))
    }

    #[tracing::instrument]
    fn create_dynamic_reload_handler() -> Result<DynamicReload,CreatePluginError> {

        let dylib_parent = Self::default_plugin_dylib_parent()?;

        let dylib_parent_str = dylib_parent
            .as_os_str()
            .to_str()
            .ok_or(
                CreatePluginError::BadFileString
            )?;

        let shadow_dir   = Self::default_plugin_shadow_dir()?;

        let shadow_dir_str = shadow_dir
            .as_os_str()
            .to_str()
            .ok_or(CreatePluginError::BadFileString)?;

        let search_paths = vec![
            dylib_parent_str
        ];

        let debounce_duration = Duration::from_secs(2);

        // Setup the reload handler. A temporary
        // directory will be created inside the
        // target/debug where plugins will be
        // loaded from. 
        //
        // That is because on some OS:es loading
        // a shared lib will lock the file so we
        // can't overwrite it so this works around
        // that issue.
        Ok(
            DynamicReload::new(
                Some(search_paths),
                Some(shadow_dir_str),
                Search::Default,
                debounce_duration,
            )
        )
    }

    #[tracing::instrument]
    fn get_entrypoint<'a>(lib: &'a dynamic_reload::Lib) -> Result<dynamic_reload::Symbol<'a, CreateKlebsFixBabyRustPlugin>, CreatePluginError> {

        tracing::debug!("get_entrypoint");

        let name = Self::default_entrypoint_symbol_name();

        unsafe {
            
            lib.lib.get(name).map_err(|e| {
                CreatePluginError::NoEntryPoint { error: e }
            })
        }
    }

    #[tracing::instrument]
    fn create_plugin_from_library_entrypoint(lib: &dynamic_reload::Lib) -> Result<Box<dyn KlebsFixBabyRustPlugin>,CreatePluginError> {

        tracing::debug!("create_plugin_from_library_entrypoint");

        Self::get_entrypoint(lib).and_then(|entrypoint| {

            let plugin = unsafe { Box::from_raw(entrypoint()) };

            Ok(plugin)
        })
    }

    #[tracing::instrument]
    fn default_entrypoint_symbol_name() -> &'static [u8] {
        b"create_klebs_fix_baby_rust_plugin"
    }

    #[tracing::instrument]
    fn default_plugin_dylib_path() -> Result<PathBuf,CreatePluginError> {

        let env = std::env::var("KLEBS_FIX_BABY_RUST_PLUGIN_PATH");

        Ok(
            PathBuf::from(env.map_err(|_e| { CreatePluginError::EnvNotSet })?)
        )
    }

    #[tracing::instrument]
    fn default_plugin_dylib_parent() -> Result<PathBuf,CreatePluginError> {

        Self::default_plugin_dylib_path()?
            .parent()
            .ok_or(CreatePluginError::NoDylibParentDir)
            .map(|s| s.to_path_buf())
    }

    #[tracing::instrument]
    fn default_plugin_shadow_dir() -> Result<PathBuf,CreatePluginError> {

        let parent = Self::default_plugin_dylib_parent()?;

        let mut buf = parent;

        buf.push("shadow");

        if !buf.exists() {
            std::fs::create_dir_all(&buf).map_err(|err|{
                CreatePluginError::CouldNotCreateShadowDir {
                    error: err
                }
            })?;
        }

        Ok(buf)
    }
}

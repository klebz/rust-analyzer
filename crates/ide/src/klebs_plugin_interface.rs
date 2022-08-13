use std::sync::{LockResult,MutexGuard,Arc,Mutex};
use std::path::PathBuf;
use derivative::*;

use chomper_plugin::*;

#[allow(dead_code)]
#[derive(Debug)]
pub enum CreatePluginError {
    EnvNotSet,
    DefaultPluginDylibPathNotSet,
    CouldNotLoadBuildCache,
    CannotCreateTempLibrary {
        error: hotlib::CreateTempLibraryError,
    },
    NoEntryPoint {
        error: hotlib::libloading::Error,
    },
    HotlibWatchFailedOnPath {
        path:  PathBuf,
        error: hotlib::WatchError,
    },
    CannotLoadLibrary {
        error: hotlib::LoadError,
    },
    CannotBuildLibrary {
        error: hotlib::BuildError,
    },
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct KlebsPluginInterface {

    #[derivative(Debug="ignore")]
    lib:        Arc<Mutex<hotlib::TempLibrary>>,

    #[derivative(Debug="ignore")]
    plugin: Arc<Mutex<Box<dyn KlebsFixBabyRustPlugin>>>,

    checksum:   Arc<Mutex<String>>,
}

impl KlebsPluginInterface {

    pub fn plugin<'a>(&'a self) -> LockResult<MutexGuard<'a, Box<dyn KlebsFixBabyRustPlugin>>>  {
        self.plugin.lock()
    }

    #[tracing::instrument]
    pub fn new() -> Result<Self, CreatePluginError> {

        let lib = Self::try_load_library_directly_from_prebuilt_dylib()?;

        let plugin  = Self::create_plugin_from_library_entrypoint(&lib)?;

        let checksum = Self::try_load_dylib_checksum()?;

        Ok(
            Self {
                lib:      Arc::new(Mutex::new(lib)),
                plugin:   Arc::new(Mutex::new(plugin)),
                checksum: Arc::new(Mutex::new(checksum)),
            }
        )
    }

    #[tracing::instrument]
    pub fn maybe_reload_plugin(&mut self) {

        tracing::debug!("maybe_reload_plugin");

        match Self::try_load_dylib_checksum() {

            Ok(new_checksum) => {

                if let Ok(old_checksum) = self.checksum.lock() {

                    if *old_checksum == new_checksum {

                        tracing::debug!(
                            "new checksum {:?} same as the old {:?} -- no need to reload", 
                            new_checksum, 
                            *old_checksum
                        );

                        return;
                    }
                }

                match self.reload_plugin() {
                    Ok(_) => {},
                    Err(e) => {
                        tracing::debug!("ERROR: reload plugin failure! {:?}", e);
                    }
                }
            },
            Err(e) => {
                tracing::debug!("ERROR: could not get dylib checksum! {:?}", e);
            }
        }
    }
}

//--------------------------------[ keep private ]
impl KlebsPluginInterface {

    #[tracing::instrument]
    fn reload_plugin(&mut self) -> Result<(), CreatePluginError> {

        tracing::debug!("reload_plugin");

        let lib    = Self::try_load_library_directly_from_prebuilt_dylib()?;

        let plugin = Self::create_plugin_from_library_entrypoint(&lib)?;

        let checksum = Self::try_load_dylib_checksum()?;

        if let Ok(mut lib_guard) = self.lib.lock() {
            *lib_guard = lib;
        }

        if let Ok(mut plugin_guard) = self.plugin.lock() {
            *plugin_guard = plugin;
        }

        if let Ok(mut checksum_guard) = self.checksum.lock() {
            *checksum_guard = checksum;
        }

        Ok(())
    }

    #[tracing::instrument]
    fn try_load_library_directly_from_prebuilt_dylib() -> Result<hotlib::TempLibrary,CreatePluginError> {

        tracing::debug!("try_load_library_directly_from_prebuild_dylib");

        if let Some(ref path) = Self::default_plugin_dylib_path() {

            hotlib::TempLibrary::new(path, "chomper2").map_err(|error| {
                CreatePluginError::CannotCreateTempLibrary {
                    error,
                }
            })

        } else {
            Err(CreatePluginError::DefaultPluginDylibPathNotSet)
        }
    }

    #[tracing::instrument]
    fn try_load_dylib_checksum() -> Result<String,CreatePluginError> {

        tracing::debug!("try_load_dylib_checksum");

        if let Some(ref path) = Self::default_plugin_dylib_path() {

            Ok(checksums::hash_file(path, checksums::Algorithm::CRC64))

        } else {
            Err(CreatePluginError::DefaultPluginDylibPathNotSet)
        }
    }

    #[tracing::instrument]
    fn get_entrypoint<'a>(lib: &'a hotlib::TempLibrary) -> Result<hotlib::Symbol<'a, CreateKlebsFixBabyRustPlugin>, CreatePluginError> {

        tracing::debug!("get_entrypoint");

        let name = Self::default_entrypoint_symbol_name();

        unsafe {
            lib.lib().get(name).map_err(|e| {
                CreatePluginError::NoEntryPoint { error: e }
            })
        }
    }

    #[tracing::instrument]
    fn create_plugin_from_library_entrypoint(lib: &hotlib::TempLibrary) -> Result<Box<dyn KlebsFixBabyRustPlugin>,CreatePluginError> {

        tracing::debug!("create_plugin_from_library_entrypoint");

        Self::get_entrypoint(lib).and_then(|entrypoint| {

            let plugin = unsafe { Box::from_raw(entrypoint()) };

            Ok(plugin)
        })
    }

    //---------------------------------------
    #[tracing::instrument]
    fn default_entrypoint_symbol_name() -> &'static [u8] {
        b"create_klebs_fix_baby_rust_plugin"
    }

    #[tracing::instrument]
    fn default_plugin_dylib_path() -> Option<PathBuf> {
        std::env::var("KLEBS_FIX_BABY_RUST_PLUGIN_PATH").ok().map(|s| PathBuf::from(s))
    }
}

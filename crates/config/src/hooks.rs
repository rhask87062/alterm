use mlua::Lua;
use std::path::Path;

pub struct LuaHooks {
    lua: Lua,
    loaded: bool,
}

impl LuaHooks {
    pub fn new() -> Self {
        Self {
            lua: Lua::new(),
            loaded: false,
        }
    }

    /// Load hooks from a Lua file. Returns Ok(true) if file exists and loaded,
    /// Ok(false) if file doesn't exist, Err on parse/runtime error.
    pub fn load_file(&mut self, path: &Path) -> Result<bool, String> {
        if !path.exists() {
            return Ok(false);
        }
        let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.load_str(&source)?;
        Ok(true)
    }

    /// Load hooks from a Lua source string. Seeds globals, then executes.
    pub fn load_str(&mut self, src: &str) -> Result<(), String> {
        self.setup_globals()?;
        self.lua
            .load(src)
            .exec()
            .map_err(|e| format!("Lua error: {e}"))?;
        self.loaded = true;
        Ok(())
    }

    /// Pre-seed the Lua environment with useful globals before loading user script.
    fn setup_globals(&self) -> Result<(), String> {
        let globals = self.lua.globals();

        let alt_table = self.lua.create_table().map_err(|e| e.to_string())?;
        alt_table
            .set("version", env!("CARGO_PKG_VERSION"))
            .map_err(|e| e.to_string())?;
        alt_table
            .set("platform", std::env::consts::OS)
            .map_err(|e| e.to_string())?;
        if let Some(home) = dirs::home_dir() {
            alt_table
                .set("home", home.to_string_lossy().to_string())
                .map_err(|e| e.to_string())?;
        }
        globals
            .set("alterm", alt_table)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Call a Lua function by name with no arguments, returning a string result.
    pub fn call_hook(&self, name: &str) -> Option<String> {
        if !self.loaded {
            return None;
        }
        let globals = self.lua.globals();
        let func: mlua::Function = globals.get(name).ok()?;
        func.call::<String>(()).ok()
    }

    /// Call a Lua function with a string argument, returning a string result.
    pub fn call_hook_with(&self, name: &str, arg: &str) -> Option<String> {
        if !self.loaded {
            return None;
        }
        let globals = self.lua.globals();
        let func: mlua::Function = globals.get(name).ok()?;
        func.call::<String>(arg.to_string()).ok()
    }

    /// Check if a hook function exists.
    pub fn has_hook(&self, name: &str) -> bool {
        if !self.loaded {
            return false;
        }
        self.lua.globals().get::<mlua::Function>(name).is_ok()
    }

    /// Get a global string variable from Lua.
    pub fn get_string(&self, name: &str) -> Option<String> {
        if !self.loaded {
            return None;
        }
        self.lua.globals().get::<String>(name).ok()
    }

    /// Get a global boolean variable from Lua.
    pub fn get_bool(&self, name: &str) -> Option<bool> {
        if !self.loaded {
            return None;
        }
        self.lua.globals().get::<bool>(name).ok()
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
}

impl Default for LuaHooks {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(src: &str) -> LuaHooks {
        let mut h = LuaHooks::new();
        h.load_str(src).unwrap();
        h
    }

    #[test]
    fn on_startup_returns_string() {
        let h = loaded("function on_startup() return 'hi ' .. alterm.version end");
        let out = h.call_hook("on_startup").unwrap();
        assert!(out.starts_with("hi "));
    }

    #[test]
    fn on_new_terminal_detected_and_returns() {
        let h = loaded("function on_new_terminal() return 'echo hello' end");
        assert!(h.has_hook("on_new_terminal"));
        assert_eq!(h.call_hook("on_new_terminal").unwrap(), "echo hello");
    }

    #[test]
    fn on_theme_change_receives_arg() {
        let h = loaded("function on_theme_change(t) return 'now ' .. t end");
        assert_eq!(h.call_hook_with("on_theme_change", "light").unwrap(), "now light");
    }

    #[test]
    fn missing_hook_returns_none() {
        let h = loaded("x = 1");
        assert!(h.call_hook("on_startup").is_none());
        assert!(!h.has_hook("on_startup"));
    }
}

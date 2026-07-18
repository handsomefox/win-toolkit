//! The Windows Sandbox configuration model and `.wsb` rendering. Portable and
//! unit-tested; the app writes the rendered file and launches it.

/// A Windows Sandbox configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxConfig {
    /// Memory to allocate to the sandbox, in megabytes.
    pub memory_mb: u32,
    /// Whether the virtual GPU is enabled.
    pub vgpu: bool,
    /// Whether networking is enabled.
    pub networking: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_mb: 4096,
            vgpu: false,
            networking: true,
        }
    }
}

impl SandboxConfig {
    /// Renders the configuration as a `.wsb` XML document.
    #[must_use]
    pub fn to_wsb(&self) -> String {
        let flag = |enabled: bool| if enabled { "Enable" } else { "Disable" };
        format!(
            "<Configuration>\n  <VGpu>{}</VGpu>\n  <Networking>{}</Networking>\n  \
             <MemoryInMB>{}</MemoryInMB>\n</Configuration>\n",
            flag(self.vgpu),
            flag(self.networking),
            self.memory_mb,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_enable_disable_flags_and_memory() {
        let config = SandboxConfig {
            memory_mb: 8192,
            vgpu: true,
            networking: false,
        };
        let wsb = config.to_wsb();
        assert!(wsb.contains("<VGpu>Enable</VGpu>"));
        assert!(wsb.contains("<Networking>Disable</Networking>"));
        assert!(wsb.contains("<MemoryInMB>8192</MemoryInMB>"));
    }
}

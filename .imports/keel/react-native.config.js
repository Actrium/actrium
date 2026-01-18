// Native sandbox is optional - most users only need rill/sdk (pure JS SDK)
// Set platforms to null to disable auto-linking
// For native sandbox users (RN): manually add `pod 'RillSandboxNative', :path => '../node_modules/rill/native'`
// and follow docs/NATIVE_SANDBOX_INTEGRATION.zh.md
module.exports = {
  dependency: {
    platforms: {
      ios: null, // Disable iOS auto-linking
      macos: null, // Disable macOS auto-linking
      android: null, // Disable Android auto-linking
    },
  },
};

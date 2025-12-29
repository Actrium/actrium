// Native sandbox is optional - most users only need rill/let (pure JS SDK)
// Set platforms to null to disable auto-linking
// For native sandbox users: manually add RillSandboxNative.podspec to Podfile
module.exports = {
  dependency: {
    platforms: {
      ios: null, // Disable iOS auto-linking
      macos: null, // Disable macOS auto-linking
      android: null, // Disable Android auto-linking
    },
  },
};

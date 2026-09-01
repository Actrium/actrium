// React Native autolinking configuration for KeelSandboxNative.
//
// Keep only Android explicit fields. iOS autolinking uses the podspec
// discovered from package root.
module.exports = {
  dependency: {
    platforms: {
      android: {
        packageImportPath: 'import com.keel.sandbox.KeelSandboxNativePackage;',
        packageInstance: 'new KeelSandboxNativePackage()',
      },
    },
  },
};

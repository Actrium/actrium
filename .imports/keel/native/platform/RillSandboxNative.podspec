require 'json'

# Read package.json from project root (two levels up from platform/)
package_json_path = File.expand_path('../../package.json', __dir__)
package = JSON.parse(File.read(package_json_path))

Pod::Spec.new do |s|
  # CocoaPods target/pod name must be a simple identifier (no @ or /)
  s.name         = "RillSandboxNative"
  s.version      = package['version']
  s.summary      = package['description']
  s.homepage     = package['homepage'] || "https://github.com/GoAskAway/rill"
  s.license      = package['license'] || "Apache-2.0"
  s.authors      = package['authors'] || { "Rill Team" => "team@rill.dev" }
  s.platforms    = { :ios => "13.0", :osx => "10.15", :tvos => "13.0", :visionos => "1.0" }
  s.source       = { :git => package['repository'] || "https://github.com/GoAskAway/rill.git", :tag => "v#{s.version}" }

  # Paths relative to podspec location (platform/)
  # Use .. to go up one level to native/
  s.source_files = "../jsi/**/*.{h,cpp}"
  s.header_mappings_dir = "../jsi"

  # QuickJS subspec - available on all platforms
  s.subspec 'QuickJS' do |quickjs|
    quickjs.source_files = [
      "../quickjs/src/**/*.{h,cpp}",
      "../quickjs/vendor/**/*.{h,c}"
    ]
    quickjs.private_header_files = "../quickjs/vendor/**/*.h"
    quickjs.compiler_flags = '-DCONFIG_VERSION="2024-01-01" -DCONFIG_BIGNUM=1'
    quickjs.pod_target_xcconfig = {
      'CLANG_CXX_LANGUAGE_STANDARD' => 'c++17',
      'GCC_WARN_INHIBIT_ALL_WARNINGS' => 'YES'
    }
  end

  # JSC subspec - Apple platforms only (uses system JavaScriptCore)
  s.subspec 'JSC' do |jsc|
    jsc.source_files = [
      "../jsc/src/**/*.{h,mm}",
      "../jsc/src/RillSandboxNativeTurboModule.mm"
    ]
    jsc.frameworks = "JavaScriptCore"
    jsc.pod_target_xcconfig = {
      'CLANG_CXX_LANGUAGE_STANDARD' => 'c++17'
    }
  end

  # Default includes both
  s.default_subspecs = ['JSC']

  # React Native dependencies for JSI and bridging
  s.dependency "React-jsi"
  s.dependency "React-RCTFoundation"
  s.dependency "React-Core"

  # Optional: React-bridging for new architecture TurboModule support
  # s.dependency "React-bridging"
end

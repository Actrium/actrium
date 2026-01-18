require 'json'

# Read package.json from project root (one level up from native/)
package_json_path = File.expand_path('../package.json', __dir__)
package = JSON.parse(File.read(package_json_path))

# Sandbox engine selection: 'jsc' (default), 'hermes', or 'quickjs'
sandbox_engine = ENV['RILL_SANDBOX_ENGINE'] || 'jsc'

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

  # Common source files (TurboModule entry point)
  # Note: We do NOT include bundled jsi files - they conflict with React-jsi
  common_sources = [
    "core/src/RillSandboxNativeTurboModule.{h,mm}"
  ]

  # Engine-specific source files
  if sandbox_engine == 'quickjs'
    s.source_files = common_sources + [
      "quickjs/src/*.{h,cpp}",
      "quickjs/vendor/*.{h,c}"
    ]
    s.exclude_files = [
      "quickjs/src/EmscriptenBindings.cpp",
      "quickjs/src/wasm_bindings.c"
    ]
    s.public_header_files = [
      "core/src/RillSandboxNativeTurboModule.h",
      "quickjs/src/*.h",
      "quickjs/vendor/*.h"
    ]
    # QuickJS is built from source, no external dependency
  elsif sandbox_engine == 'hermes'
    s.source_files = common_sources + [
      "hermes/src/**/*.{h,mm}"
    ]
    s.public_header_files = [
      "core/src/RillSandboxNativeTurboModule.h",
      "hermes/src/**/*.h"
    ]
    # Hermes sandbox needs hermes-engine from React Native (not CocoaPods trunk which is outdated 0.11.0)
    # The app's Podfile must either:
    # 1. Enable Hermes as main runtime (:hermes_enabled => true), or
    # 2. Manually add: pod 'hermes-engine', :path => "#{react_native_path}/sdks/hermes-engine"
    # We don't add dependency here to avoid pulling outdated 0.11.0 from trunk
  else
    # Default: JSC sandbox
    s.source_files = common_sources + [
      "jsc/src/**/*.{h,mm}"
    ]
    s.public_header_files = [
      "core/src/RillSandboxNativeTurboModule.h",
      "jsc/src/**/*.h"
    ]
  end
  s.static_framework = true
  s.requires_arc = true

  # React Native dependencies for JSI and bridging
  s.dependency "React-jsi"
  s.dependency "React-Core"

  # Detect new architecture from environment for logging/debugging
  new_arch_enabled = ENV['RCT_NEW_ARCH_ENABLED'] == '1'

  # Build preprocessor definitions
  preprocessor_defs = '$(inherited)'
  if new_arch_enabled
    preprocessor_defs += ' RCT_NEW_ARCH_ENABLED=1'
  end

  # Set sandbox engine preprocessor define
  if sandbox_engine == 'quickjs'
    preprocessor_defs += ' RILL_SANDBOX_ENGINE=3'  # RILL_SANDBOX_ENGINE_QUICKJS
  elsif sandbox_engine == 'hermes'
    preprocessor_defs += ' RILL_SANDBOX_ENGINE=2'  # RILL_SANDBOX_ENGINE_HERMES
  else
    preprocessor_defs += ' RILL_SANDBOX_ENGINE=1'  # RILL_SANDBOX_ENGINE_JSC
  end

  s.pod_target_xcconfig = {
    'GCC_PREPROCESSOR_DEFINITIONS' => preprocessor_defs
  }

  # Installation mechanisms:
  # - Bridgeless / New arch: Auto via RCTHost runtime hook (fallback: App can call RillSandboxNativeInstall()).
  # - Legacy bridge: Auto via RCTJavaScriptWillStartExecutingNotification.
end

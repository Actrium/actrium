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

  # Copy symlinked source files to a local directory for CocoaPods
  # CocoaPods requires relative paths and doesn't always follow symlinks
  s.prepare_command = <<-CMD
    mkdir -p Sources/jsi Sources/jsc
    cp -RL jsi/* Sources/jsi/ 2>/dev/null || true
    cp -RL jsc/src/* Sources/jsc/ 2>/dev/null || true
  CMD

  # Reference copied files with relative paths
  s.source_files = [
    "Sources/jsi/**/*.{h,cpp}",
    "Sources/jsc/**/*.{h,mm}"
  ]
  s.public_header_files = [
    "Sources/jsi/**/*.h",
    "Sources/jsc/**/*.h"
  ]

  s.header_mappings_dir = "Sources/jsi"
  s.static_framework = true
  s.requires_arc = true

  # React Native dependencies for JSI and bridging
  s.dependency "React-jsi"
  s.dependency "React-Core"

  # Note: TurboModule bridging is optional and only available in new arch (RN 0.83+)
  # For old arch (RN 0.79 macOS), JSI is installed via RCTJavaScriptWillStartExecutingNotification
end

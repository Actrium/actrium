const fs = require('fs');
const path = require('path');

const packageRoot = path.join(__dirname, '..', 'node_modules', 'react-native-macos');
const runtimePropsOnly = process.argv.includes('--runtime-props-only');

function patchFile(relativePath, patcher, required = false) {
  const filePath = path.join(packageRoot, relativePath);
  if (!fs.existsSync(filePath)) {
    if (required) throw new Error(`Missing React Native macOS source: ${filePath}`);
    return;
  }

  const before = fs.readFileSync(filePath, 'utf8');
  const after = patcher(before);
  if (after !== before) {
    fs.writeFileSync(filePath, after);
  }
}

// RuntimeApp surface props can contain browsing history and bookmarks.
patchFile('Libraries/ReactNative/AppRegistryImpl.js', (source) => {
  const logGuard = "if (appKey !== 'LogBox') {";
  const redactedGuard = "if (appKey !== 'LogBox' && appKey !== 'RuntimeApp') {";
  const unpatched = source.split(logGuard).length - 1;
  const patched = source.split(redactedGuard).length - 1;
  if (unpatched + patched !== 2) {
    throw new Error('React Native macOS AppRegistry logging changed. Update the RuntimeApp props redaction.');
  }
  return source.replaceAll(logGuard, redactedGuard);
}, runtimePropsOnly);

if (runtimePropsOnly) process.exit(0);

patchFile('ReactCommon/hermes/executor/HermesExecutorFactory.cpp', (source) => {
  if (source.includes('#include <thread>')) {
    return source;
  }
  return source.replace(
    '#include "HermesExecutorFactory.h"\n',
    '#include "HermesExecutorFactory.h"\n\n#include <thread>\n',
  );
});

patchFile('ReactCommon/react/runtime/hermes/HermesInstance.cpp', (source) => {
  if (source.includes('#if !TARGET_OS_OSX')) {
    return source.replace('    #if !TARGET_OS_OSX', '#if !TARGET_OS_OSX');
  }
  return source.replace(
    '  void unstable_initializeOnJsThread() override {\n    runtime_->registerForProfiling();\n  }\n',
    '  void unstable_initializeOnJsThread() override {\n#if !TARGET_OS_OSX\n    runtime_->registerForProfiling();\n#endif\n  }\n',
  );
});

patchFile('scripts/codegen/generate-artifacts-executor/utils.js', (source) =>
  source.replace(
    'if (dependency === REACT_NATIVE) {',
    "if (dependency === REACT_NATIVE || dependency === 'react-native') {",
  ),
);

patchFile('React/Fabric/Mounting/ComponentViews/ScrollView/RCTScrollViewComponentView.mm', (source) => {
  const flexible =
    '    _containerView.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;';
  const fixed = [
    '    // Fabric sets the document size before laying out the viewport.',
    '    // Keep AppKit from adding the viewport size to the scrollable content.',
    '    _containerView.autoresizingMask = NSViewNotSizable;',
  ].join('\n');

  // macOS delivers NSClipView bounds changes through RCTUIScrollViewDelegate.
  // Fabric needs the same callback as iOS to update the shadow tree's contentOffset.
  const register = '    [self.scrollViewDelegateSplitter addDelegate:self];';
  const edits = [
    [flexible, fixed],
    ['#if !TARGET_OS_OSX // [macOS]\n    UIScrollViewDelegate,\n#endif // [macOS]', '    RCTUIScrollViewDelegate,'],
    [`#if !TARGET_OS_OSX // [macOS]\n${register}\n#endif // [macOS]\n#if TARGET_OS_IOS`, `${register}\n#if TARGET_OS_IOS`],
  ];
  for (const [before, after] of edits) {
    if (source.includes(before)) {
      source = source.replace(before, after);
    } else if (!source.includes(after)) {
      throw new Error(`Unrecognized macOS Fabric ScrollView source while applying:\n${after}`);
    }
  }
  return source;
});

const aliasPath = path.join(
  packageRoot,
  'src',
  'private',
  'inspector',
  'getInspectorDataForViewAtPoint.js',
);
const sourcePath = path.join(
  packageRoot,
  'src',
  'private',
  'devsupport',
  'devmenu',
  'elementinspector',
  'getInspectorDataForViewAtPoint.js',
);

if (fs.existsSync(sourcePath)) {
  fs.mkdirSync(path.dirname(aliasPath), { recursive: true });
  fs.writeFileSync(
    aliasPath,
    "export {default} from '../devsupport/devmenu/elementinspector/getInspectorDataForViewAtPoint';\n",
  );
}

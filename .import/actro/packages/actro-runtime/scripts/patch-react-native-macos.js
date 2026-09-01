const fs = require('fs');
const path = require('path');

const packageRoot = path.join(__dirname, '..', 'node_modules', 'react-native-macos');

function patchFile(relativePath, patcher) {
  const filePath = path.join(packageRoot, relativePath);
  if (!fs.existsSync(filePath)) {
    return;
  }

  const before = fs.readFileSync(filePath, 'utf8');
  const after = patcher(before);
  if (after !== before) {
    fs.writeFileSync(filePath, after);
  }
}

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

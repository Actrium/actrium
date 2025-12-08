#!/usr/bin/env node
/**
 * Rill CLI
 *
 * Plugin bundler tool
 */

import { program } from 'commander';
import type { BuildOptions } from './build';
import { build } from './build';
import { version } from '../package.json';

program
  .name('rill')
  .description('Rill plugin CLI - Build dynamic UI plugins for React Native')
  .version(version);

program
  .command('build')
  .description('Build a plugin bundle')
  .argument('<entry>', 'Entry file path (e.g., src/plugin.tsx)')
  .option('-o, --outfile <path>', 'Output file path', 'dist/bundle.js')
  .option('--no-minify', 'Disable minification')
  .option('--sourcemap', 'Generate sourcemap')
  .option('--watch', 'Watch mode for development')
  .option('--metafile <path>', 'Output build metadata to file')
  .action(async (entry: string, options: Partial<BuildOptions>) => {
    try {
      await build({
        entry,
        outfile: options.outfile ?? 'dist/bundle.js',
        minify: options.minify ?? true,
        sourcemap: options.sourcemap ?? false,
        watch: options.watch ?? false,
        metafile: options.metafile,
      });
    } catch (error) {
      console.error('Build failed:', error);
      process.exit(1);
    }
  });

program
  .command('init')
  .description('Initialize a new plugin project')
  .argument('[name]', 'Project name', 'my-rill-plugin')
  .action(async (name: string) => {
    const fs = await import('fs');
    const path = await import('path');
    const targetDir = path.resolve(process.cwd(), name);

    if (!fs.existsSync(targetDir)) {
      fs.mkdirSync(targetDir, { recursive: true });
    }

    const srcDir = path.join(targetDir, 'src');
    if (!fs.existsSync(srcDir)) fs.mkdirSync(srcDir, { recursive: true });

    // package.json
    const pkgPath = path.join(targetDir, 'package.json');
    if (!fs.existsSync(pkgPath)) {
      const pkg = {
        name,
        version: '0.1.0',
        private: true,
        scripts: {
          build: 'rill build src/plugin.tsx -o dist/bundle.js',
          analyze: 'rill analyze dist/bundle.js'
        },
        devDependencies: {
          typescript: '^5.0.0'
        },
        dependencies: {
          react: '^18.0.0',
          'react/jsx-runtime': '^18.0.0',
          'rill': 'latest',
          'rill/sdk': 'latest'
        }
      };
      fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2));
    }

    // tsconfig.json
    const tsconfigPath = path.join(targetDir, 'tsconfig.json');
    if (!fs.existsSync(tsconfigPath)) {
      fs.writeFileSync(tsconfigPath, JSON.stringify({
        compilerOptions: {
          target: 'ES2020',
          module: 'ESNext',
          jsx: 'react-jsx',
          moduleResolution: 'Node',
          strict: true,
          skipLibCheck: true,
          esModuleInterop: true
        },
        include: ['src']
      }, null, 2));
    }

    // src/plugin.tsx
    const pluginPath = path.join(srcDir, 'plugin.tsx');
    if (!fs.existsSync(pluginPath)) {
      const plugin = `import * as React from 'react';
import { View, Text } from 'react-native';
import { withRill } from 'rill/sdk';

function Plugin() {
  return (
    <View>
      <Text>Hello from Rill Plugin</Text>
    </View>
  );
}

export default withRill(Plugin);
`;
      fs.writeFileSync(pluginPath, plugin);
    }

    console.log(`Initialized plugin project at ${targetDir}`);
    console.log('Next steps:');
    console.log(`  cd ${name}`);
    console.log('  npm install');
    console.log('  npx rill build src/plugin.tsx -o dist/bundle.js');
  });

program.parse();

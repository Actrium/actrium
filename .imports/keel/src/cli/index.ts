#!/usr/bin/env node

/**
 * Rill CLI
 *
 * Plugin bundler tool
 */

import { program } from 'commander';
import { build, BuildOptions } from './build';
import { version } from '../../package.json';

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
    console.log(`Initializing new plugin project: ${name}`);
    // TODO: Implement project initialization scaffold
    console.log('This feature is coming soon!');
  });

program.parse();

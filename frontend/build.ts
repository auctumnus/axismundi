import { readdir, mkdir, readFile, writeFile, watch, copyFile } from 'fs/promises';
import { join, dirname, relative, extname, basename } from 'path';
import { existsSync } from 'fs';
import { transform } from 'lightningcss';
import { transform as swcTransform } from '@swc/core';

const srcDir = join(import.meta.dir, 'src');
const distDir = join(import.meta.dir, 'dist');

async function ensureDir(dir: string) {
  try {
    await mkdir(dir, { recursive: true });
  } catch (err: any) {
    if (err.code !== 'EEXIST') throw err;
  }
}

async function getAllFiles(dir: string, files: string[] = []): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = join(dir, entry.name);
    if (entry.isDirectory()) {
      await getAllFiles(fullPath, files);
    } else {
      files.push(fullPath);
    }
  }

  return files;
}

async function processCSS(inputPath: string, outputPath: string) {
  const css = await readFile(inputPath, 'utf8');
  const result = transform({
    code: Buffer.from(css),
    minify: true,
    sourceMap: false,
    filename: basename(inputPath)
  });

  await writeFile(outputPath, result.code);
  console.log(`Processed CSS: ${relative(process.cwd(), inputPath)} → ${relative(process.cwd(), outputPath)}`);
}

async function processJS(inputPath: string, outputPath: string) {
  const code = await readFile(inputPath, 'utf8');

  const result = await swcTransform(code, {
    filename: inputPath,
    sourceMaps: true,
    jsc: {
      target: 'es2020',
      parser: {
        syntax: 'typescript',
      },
    },
    module: {
      type: 'es6',
    },
    minify: true,
  });

  await writeFile(outputPath, result.code);
  await writeFile(outputPath + '.map', result.map || '');
  console.log(`Processed JS: ${relative(process.cwd(), inputPath)} → ${relative(process.cwd(), outputPath)}`);
}

// Entry points that need bundling (React components and main entry)
const BUNDLE_ENTRY_POINTS = [
  'src/form-editor.tsx',
  'src/annotation.tsx',
  'src/etymology-modal.tsx',
  'src/word-combobox.tsx',
  'src/user-combobox.tsx',
  'src/phonology-editor.tsx',
  'src/definitions-editor.tsx',
  'src/sound-change-runner.ts',
  'src/sound-change-sets/view.ts',
  'src/tooltip.ts',
];

async function bundleReactFiles() {
  const entrypoints = BUNDLE_ENTRY_POINTS.map(entry => join(import.meta.dir, entry));

  // Filter to only existing files
  const existingEntrypoints = entrypoints.filter(entry => existsSync(entry));

  if (existingEntrypoints.length === 0) {
    return;
  }

  const result = await Bun.build({
    entrypoints: existingEntrypoints,
    outdir: distDir,
    root: srcDir,
    minify: true,
    sourcemap: 'linked',
    target: 'browser',
    format: 'esm',
  });

  if (!result.success) {
    console.error('Bundle errors:');
    for (const message of result.logs) {
      console.error(message);
    }
    throw new Error('Bundle failed');
  }

  for (const entry of existingEntrypoints) {
    const relativePath = relative(import.meta.dir, entry);
    console.log(`Bundled: ${relativePath} → dist/${basename(entry, extname(entry))}.js`);
  }
}

async function processFile(filePath: string) {
  const relativePath = relative(srcDir, filePath);
  const ext = extname(filePath);
  const baseName = basename(filePath, ext);

  // Skip TypeScript declaration files
  if (filePath.endsWith('.d.ts')) {
    return;
  }

  // skip tests
  if (filePath.endsWith('.test.ts') || filePath.endsWith('.test.tsx')) {
    return;
  }

  // Skip files that are in the bundle entry points
  const relativeFromRoot = relative(import.meta.dir, filePath);
  if (BUNDLE_ENTRY_POINTS.includes(relativeFromRoot)) {
    return;
  }

  // Skip all .tsx files - they will be bundled
  if (ext === '.tsx') {
    return;
  }

  // Create output directory structure
  const outputDir = join(distDir, dirname(relativePath));
  await ensureDir(outputDir);

  try {
    if (ext === '.css') {
      const outputPath = join(outputDir, `${baseName}.css`);
      await processCSS(filePath, outputPath);
    } else if (ext === '.ts' || ext === '.js') {
      const outputPath = join(outputDir, `${baseName}.js`);
      await processJS(filePath, outputPath);
    } else {
      console.log(`Skipping ${relativePath} (will be bundled or not needed)`);
    }
  } catch (err: any) {
    console.error(`Error processing ${relativePath}:`, err.message);
  }
}

async function build() {
  console.log('Starting build...');

  // Ensure dist directory exists
  await ensureDir(distDir);

  // Bundle React files first
  await bundleReactFiles();

  // Get all files in src
  const files = await getAllFiles(srcDir);

  // Process non-React files
  for (const file of files) {
    await processFile(file);
  }

  console.log('Build complete!');
}

async function watchFiles() {
  console.log('Starting watch mode...');

  // Initial build
  await build();

  console.log(`Watching ${srcDir} for changes...`);

  try {
    const watcher = watch(srcDir, { recursive: true });

    for await (const event of watcher) {
      if (event.filename) {
        const filePath = join(srcDir, event.filename);

        // Only process if file exists (not deleted)
        if (existsSync(filePath)) {
          console.log(`\nFile changed: ${event.filename}`);

          // If a bundled file changed, rebuild all bundles
          const isBundledFile = BUNDLE_ENTRY_POINTS.some(entry =>
            filePath.endsWith(entry.replace('src/', ''))
          ) || filePath.endsWith('.tsx');

          if (isBundledFile) {
            await bundleReactFiles();
          } else {
            await processFile(filePath);
          }
        }
      }
    }
  } catch (err: any) {
    console.error('Watch error:', err.message);
  }
}

// Check command line arguments
const args = process.argv.slice(2);
const isWatchMode = args.includes('--watch') || args.includes('-w');

if (isWatchMode) {
  watchFiles().catch(console.error);
} else {
  build().catch(console.error);
}

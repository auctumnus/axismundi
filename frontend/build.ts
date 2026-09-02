import { readdir, mkdir, readFile, writeFile, watch, copyFile } from 'fs/promises';
import { join, dirname, relative, extname, basename, resolve as resolvePath } from 'path';
import { existsSync } from 'fs';
import { bundleAsync } from 'lightningcss';
import { transform as swcTransform } from '@swc/core';

// CSS dependency graph: entry -> imports it pulls in, and the reverse.
// Used in watch mode to rebuild entries when an imported fragment changes.
const cssDeps = new Map<string, Set<string>>();
const cssReverseDeps = new Map<string, Set<string>>();

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

// Files imported into main.css live under a cascade layer determined by their
// folder. Wrapping the source in `@layer X { ... }` at read time spares us from
// annotating every `@import` with `layer(X)` and keeps the layering visible at
// the file level. lightningcss can't synthesize this on its own for nested
// imports, so we do it in the resolver.
function getLayerForFile(filePath: string): string | null {
  const rel = relative(srcDir, filePath);
  if (rel.startsWith('..') || rel === '' || rel.startsWith('/')) return null;
  if (rel === 'reset.css') return 'reset';
  if (rel === 'utilities.css') return 'utilities';
  const top = rel.split('/')[0];
  if (top === 'design' || top === 'layout' || top === 'components' || top === 'page') {
    return top;
  }
  return null;
}

async function processCSS(inputPath: string, outputPath: string) {
  const deps = new Set<string>();
  const result = await bundleAsync({
    filename: inputPath,
    minify: true,
    sourceMap: false,
    resolver: {
      resolve(specifier, originatingFile) {
        const resolved = resolvePath(dirname(originatingFile), specifier);
        deps.add(resolved);
        return resolved;
      },
      async read(file) {
        const source = await readFile(file, 'utf8');
        const layer = getLayerForFile(file);
        return layer ? `@layer ${layer} {\n${source}\n}\n` : source;
      },
    },
  });

  // Refresh the dep graph for this entry: clear stale reverse links, then add fresh ones.
  const previous = cssDeps.get(inputPath);
  if (previous) {
    for (const dep of previous) {
      cssReverseDeps.get(dep)?.delete(inputPath);
    }
  }
  cssDeps.set(inputPath, deps);
  for (const dep of deps) {
    let entries = cssReverseDeps.get(dep);
    if (!entries) {
      entries = new Set();
      cssReverseDeps.set(dep, entries);
    }
    entries.add(inputPath);
  }

  await writeFile(outputPath, result.code);
  console.log(`Bundled CSS: ${relative(process.cwd(), inputPath)} → ${relative(process.cwd(), outputPath)}`);
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
  'src/components/combobox/word-combobox.tsx',
  'src/components/combobox/word-categories-multiselect.tsx',
  'src/components/combobox/grammar-table-scope-multiselect.tsx',
  'src/components/combobox/user-combobox.tsx',
  'src/components/markdown-editor/markdown-editor.ts',
  'src/components/panzoom/panzoom-enhance.ts',
  'src/components/tooltip/tooltip.ts',
  'src/page/languages/phonology-editor.tsx',
  'src/page/languages/grammar-table-editor.tsx',
  'src/page/sound-changes/runner/sound-change-runner.ts',
  'src/page/sound-changes/view.ts',
  'src/page/translations/quotations-editor.tsx',
  'src/page/words/definitions-editor.tsx',
  'src/page/words/grammar-tables.ts',
  'src/main.ts'
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
    const outRelative = relative(srcDir, entry).replace(/\.tsx?$/, '.js');
    console.log(`Bundled: ${relativePath} → dist/${outRelative}`);
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

            // If a CSS file changed, also rebuild every entry that @imports it.
            if (filePath.endsWith('.css')) {
              const canonical = resolvePath(filePath);
              const dependents = cssReverseDeps.get(canonical);
              if (dependents) {
                // Snapshot: processCSS rewrites cssReverseDeps[canonical], and
                // deleting + re-adding an entry during Set iteration makes the
                // iterator revisit it — infinite loop.
                for (const entry of [...dependents]) {
                  if (entry === canonical) continue;
                  const entryRelative = relative(srcDir, entry);
                  const entryOutDir = join(distDir, dirname(entryRelative));
                  await ensureDir(entryOutDir);
                  const entryOutPath = join(entryOutDir, basename(entry));
                  await processCSS(entry, entryOutPath);
                }
              }
            }
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

import esbuild, { Plugin } from 'esbuild';
import { nodeExternalsPlugin } from 'esbuild-node-externals';

async function main() {
  const config = {
    entryPoints: ['./src/index.ts'],
    outfile: 'dist/index.js',
    bundle: true,
    platform: 'node',
    format: 'esm',
    target: 'node20',
    logLevel: 'info',
    plugins: [
      nodeExternalsPlugin({
        allowWorkspaces: true,
      }),
    ] as unknown as Plugin[],
  } satisfies esbuild.BuildOptions;

  if (process.argv.includes('--watch')) {
    const ctx = await esbuild.context(config);
    await ctx.watch();
    return;
  }

  await esbuild.build(config);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

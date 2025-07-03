import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['src/**/*.{test,spec}.ts', 'tests/**/*.{test,spec}.ts'],
    exclude: ['node_modules', 'dist'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'dist/',
        '**/*.d.ts',
        '**/*.test.ts',
        '**/*.spec.ts',
        'tests/setup.ts'
      ]
    },
    testTimeout: 15 * 60 * 1000, // 15 minutes
    globalSetup: './global-setup.ts'
  },
  resolve: {
    alias: {
      '@': '/src'
    }
  }
}); 

// Test setup file for Vitest
import { beforeAll, afterAll } from 'vitest';
import { cleanHistoricalLogs } from '../src';

// Global test setup
beforeAll(() => {
  console.log('Setting up test environment...');
  
  // Clean historical logs at the start of integration tests
  cleanHistoricalLogs('./logs');
});

// Global test cleanup
afterAll(() => {
  console.log('Cleaning up test environment...');
}); 
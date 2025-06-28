import * as fs from 'fs';
import * as path from 'path';

/**
 * Simple file-based mutex implementation for Node.js
 */
export class FileMutex {
  private lockFile: string;
  private lockDir: string;

  constructor(name: string, lockDir: string = '/tmp') {
    this.lockDir = lockDir;
    this.lockFile = path.join(lockDir, `${name}.lock`);
  }

  async acquire(): Promise<void> {
    const maxRetries = 600 * 10; // 10 minutes with 100ms intervals
    const retryDelay = 100; // 100ms

    for (let i = 0; i < maxRetries; i++) {
      try {
        // Try to create the lock file exclusively
        const fd = fs.openSync(this.lockFile, 'wx');
        fs.closeSync(fd);
        
        // Write current process info to lock file
        const processInfo = {
          pid: process.pid,
          timestamp: new Date().toISOString(),
          command: process.argv.join(' ')
        };
        fs.writeFileSync(this.lockFile, JSON.stringify(processInfo, null, 2));
        
        console.log(`🔒 Acquired mutex lock: ${this.lockFile}`);
        return;
      } catch (error: any) {
        if (error.code === 'EEXIST') {
          // Lock file exists, check if it's stale
          try {
            const lockContent = fs.readFileSync(this.lockFile, 'utf8');
            const lockInfo = JSON.parse(lockContent);
            const lockTime = new Date(lockInfo.timestamp);
            const now = new Date();
            
            // Consider lock stale if it's older than 5 minutes
            if (now.getTime() - lockTime.getTime() > 5 * 60 * 1000) {
              console.log(`⚠️  Removing stale lock: ${this.lockFile}`);
              fs.unlinkSync(this.lockFile);
              continue;
            }
          } catch (readError) {
            // If we can't read the lock file, it might be corrupted, so remove it
            try {
              fs.unlinkSync(this.lockFile);
              continue;
            } catch (unlinkError) {
              // Ignore unlink errors
            }
          }
          
          // Wait before retrying
          await new Promise(resolve => setTimeout(resolve, retryDelay));
        } else {
          throw error;
        }
      }
    }
    
    throw new Error(`Failed to acquire mutex lock after ${maxRetries} retries: ${this.lockFile}`);
  }

  release(): void {
    try {
      if (fs.existsSync(this.lockFile)) {
        fs.unlinkSync(this.lockFile);
        console.log(`🔓 Released mutex lock: ${this.lockFile}`);
      }
    } catch (error) {
      console.warn(`Warning: Failed to release mutex lock: ${this.lockFile}`, error);
    }
  }
}

/**
 * Removes all historical log files from the specified directory
 */
export function cleanHistoricalLogs(logsDir: string = './logs'): void {
  if (fs.existsSync(logsDir)) {
    const files = fs.readdirSync(logsDir);
    let removedCount = 0;
    
    for (const file of files) {
      if (file.endsWith('.log')) {
        const filePath = path.join(logsDir, file);
        try {
          fs.unlinkSync(filePath);
          removedCount++;
        } catch (error: any) {
          if (error.code !== 'ENOENT') {
            console.warn(`⚠️  Failed to remove log file ${filePath}:`, error.message);
          }
        }
      }
    }
    
    console.log(`🧹 Cleaned ${removedCount} historical log files from ${logsDir}`);
  } else {
    console.log(`📁 Logs directory ${logsDir} does not exist, nothing to clean`);
  }
}

/**
 * Removes all test chain configurations from the chains directory
 */
export function cleanTestChains(chainsDir: string = './chains'): void {
  if (!fs.existsSync(chainsDir)) {
    console.log(`📁 Chains directory ${chainsDir} does not exist, nothing to clean`);
    return;
  }

  const files = fs.readdirSync(chainsDir);
  let removedCount = 0;
  
  for (const file of files) {
    const filePath = path.join(chainsDir, file);
    try {
      const stat = fs.statSync(filePath);
      
      // Check if it's a directory and matches test chain pattern (contains underscore followed by 8 hex chars)
      if (stat.isDirectory() && /^[a-z_]+_[a-f0-9]{8}$/.test(file)) {
        fs.rmSync(filePath, { recursive: true, force: true });
        removedCount++;
        console.log(`🗑️  Removed test chain: ${file}`);
      }
    } catch (error: any) {
      if (error.code !== 'ENOENT') {
        console.warn(`⚠️  Failed to remove test chain ${file}:`, error.message);
      }
    }
  }
  
  console.log(`🧹 Cleaned ${removedCount} test chain configurations from ${chainsDir}`);
}

/**
 * Cleans up any leftover mutex lock files from previous test runs
 */
export function cleanMutexLockFiles(): void {
  const mutexLockFile = '/tmp/zkstack_chain_init_phase1.lock';
  if (fs.existsSync(mutexLockFile)) {
    try {
      fs.unlinkSync(mutexLockFile);
      console.log('🧹 Cleaned up leftover mutex lock file from previous test run');
    } catch (error) {
      console.warn('⚠️  Warning: Could not remove leftover mutex lock file:', error);
    }
  }
} 
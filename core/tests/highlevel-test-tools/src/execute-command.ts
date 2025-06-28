import {ChildProcess, spawn} from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Strips ANSI escape sequences from a string
 * @param str - The string to clean
 * @returns The string with ANSI escape sequences removed
 */
function stripAnsiEscapeCodes(str: string): string {
  // Remove ANSI escape sequences (colors, cursor movements, etc.)
  return str.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
}

/**
 * Logs a command to the executed commands log file
 * @param chainName - The chain name
 * @param command - The command being executed
 * @param args - The command arguments
 * @param startTime - The start time of the command
 * @param endTime - The end time of the command (optional for detached commands)
 * @param isDetached - Whether this is a detached/background command
 */
function logExecutedCommand(chainName: string, command: string, args: string[], startTime: number, endTime?: number, isDetached: boolean = false): void {
  const logsDir = '../../../logs/highlevel';
  
  // Ensure logs directory exists
  if (!fs.existsSync(logsDir)) {
    fs.mkdirSync(logsDir, { recursive: true });
  }
  
  // Create executed commands log file for this chain
  const executedCommandsLogFile = path.join(logsDir, `${chainName}_executed_commands.log`);
  
  const timestamp = new Date(startTime).toISOString();
  const fullCommand = `${command} ${args.join(' ')}`;
  
  let timeInfo: string;
  if (isDetached) {
    timeInfo = '(detached)';
  } else if (endTime) {
    const duration = endTime - startTime;
    timeInfo = `(${duration.toString().padStart(6)}ms)`;
  } else {
    timeInfo = '(running)';
  }
  
  // Log command with timestamp and time info, padded so commands align
  const logEntry = `[${timestamp}] ${timeInfo} ${fullCommand}\n`;
  fs.appendFileSync(executedCommandsLogFile, logEntry);
}

/**
 * Executes a command and returns a promise
 */
export async function executeCommand(command: string, args: string[], chainName: string, logFileSuffix?: string): Promise<void> {
  const logsDir = '../../../logs/highlevel';
  const startTime = Date.now();
  
  return new Promise((resolve, reject) => {
    console.log(`Executing: ${command} ${args.join(' ')}`);
    
    // Ensure logs directory exists
    if (!fs.existsSync(logsDir)) {
      fs.mkdirSync(logsDir, { recursive: true });
    }
    
    // Create log file for this chain (one file per chain, no timestamp)
    const logFileName = logFileSuffix ? `${chainName}_${logFileSuffix}.log` : `${chainName}.log`;
    const logFilePath = path.join(logsDir, logFileName);
    
    // Create write stream for the log file (append mode)
    const logStream = fs.createWriteStream(logFilePath, { flags: 'a' });
    
    // Write command to log
    const logEntry = `[${new Date().toISOString()}] Executing: ${command} ${args.join(' ')}\n`;
    logStream.write(logEntry);
    
    const child = spawn(command, args, {
      stdio: ['inherit', 'pipe', 'pipe'],
      shell: true
    });
    
    // Pipe stdout and stderr only to log file (not to console) with ANSI escape codes stripped
    child.stdout?.on('data', (data) => {
      const output = stripAnsiEscapeCodes(data.toString());
      logStream.write(output);
    });
    
    child.stderr?.on('data', (data) => {
      const output = stripAnsiEscapeCodes(data.toString());
      logStream.write(output);
    });
    
    child.on('close', (code) => {
      const endTime = Date.now();
      
      // Log the command completion to executed commands log
      logExecutedCommand(chainName, command, args, startTime, endTime, false);
      
      const closeMessage = `[${new Date().toISOString()}] Command finished with exit code: ${code}\n`;
      logStream.write(closeMessage);
      logStream.end();
      
      if (code === 0) {
        console.log(`✅ Command completed successfully. Logs saved to: ${logFilePath}`);
        resolve();
      } else {
        const errorMessage = `Command failed with exit code ${code}. Check logs at: ${logFilePath}`;
        console.error(`❌ ${errorMessage}`);
        reject(new Error(errorMessage));
      }
    });
    
    child.on('error', (error) => {
      const endTime = Date.now();
      
      // Log the command error to executed commands log
      logExecutedCommand(chainName, command, args, startTime, endTime, false);
      
      const errorMessage = `[${new Date().toISOString()}] Command error: ${error.message}\n`;
      logStream.write(errorMessage);
      logStream.end();
      reject(error);
    });
  });
}

/**
 * Executes a command in background and returns a promise that resolves immediately
 * @param command - The command to execute
 * @param args - Command arguments
 * @param chainName - Name for logging
 * @returns Promise that resolves when the command starts (not when it finishes)
 */
export async function executeBackgroundCommand(command: string, args: string[], chainName: string): Promise<ChildProcess> {
  const logsDir = '../../../logs/highlevel';
  const startTime = Date.now();
  
  return new Promise((resolve, reject) => {
    console.log(`Starting background command: ${command} ${args.join(' ')}`);
    
    // Ensure logs directory exists
    if (!fs.existsSync(logsDir)) {
      fs.mkdirSync(logsDir, { recursive: true });
    }
    
    // Create log file for this chain
    const logFileName = `${chainName}.log`;
    const logFilePath = path.join(logsDir, logFileName);
    
    // Create write stream for the log file (append mode)
    const logStream = fs.createWriteStream(logFilePath, { flags: 'a' });
    
    // Write command to log
    const logEntry = `[${new Date().toISOString()}] Starting background command: ${command} ${args.join(' ')}\n`;
    logStream.write(logEntry);
    
    const child = spawn(command, args, {
      stdio: ['inherit', 'pipe', 'pipe'],
      shell: true,
      detached: true // Run in background
    });
    
    // Pipe stdout and stderr to log file with ANSI escape codes stripped
    child.stdout?.on('data', (data) => {
      const output = stripAnsiEscapeCodes(data.toString());
      logStream.write(output);
    });
    
    child.stderr?.on('data', (data) => {
      const output = stripAnsiEscapeCodes(data.toString());
      logStream.write(output);
    });
    
    child.on('error', (error) => {
      const endTime = Date.now();
      
      // Log the command error to executed commands log
      logExecutedCommand(chainName, command, args, startTime, endTime, true);
      
      const errorMessage = `[${new Date().toISOString()}] Background command error: ${error.message}\n`;
      logStream.write(errorMessage);
      logStream.end();
      reject(error);
    });
    
    // Resolve immediately after the process starts
    child.on('spawn', () => {
      // Log the command start to executed commands log
      logExecutedCommand(chainName, command, args, startTime, undefined, true);
      
      console.log(`✅ Background command started successfully. Logs saved to: ${logFilePath}`);
      resolve(child);
    });
  });
} 

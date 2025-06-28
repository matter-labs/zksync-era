import * as fs from 'fs';
import * as path from 'path';

/**
 * Timing utility for logging step durations
 */
export class StepTimer {
  private startTime: number = 0;
  private steps: { name: string; duration: number }[] = [];
  private currentStep: string | null = null;
  private currentStepStart: number = 0;
  private logFile: string;
  private timerName: string;

  constructor(logFile: string, timerName: string = 'execution') {
    this.logFile = logFile;
    this.timerName = timerName;
    this.ensureLogDirectory();
  }

  private ensureLogDirectory(): void {
    const logDir = path.dirname(this.logFile);
    if (!fs.existsSync(logDir)) {
      fs.mkdirSync(logDir, { recursive: true });
    }
  }

  startStep(stepName: string): void {
    if (this.currentStep) {
      this.endStep(this.currentStep);
    }
    this.currentStep = stepName;
    this.currentStepStart = Date.now();
  }

  endStep(stepName: string): void {
    if (this.currentStep === stepName) {
      const duration = Date.now() - this.currentStepStart;
      this.steps.push({ name: stepName, duration });
      const timestamp = new Date().toISOString();
      const timeInfo = `(${duration.toString().padStart(6)}ms)`;
      this.log(`[${timestamp}] ${timeInfo} ${stepName}`);
      this.currentStep = null;
    }
  }

  logTotalTime(): void {
    const totalTime = this.steps.reduce((sum, step) => sum + step.duration, 0);
    const timestamp = new Date().toISOString();
    const timeInfo = `(${totalTime.toString().padStart(6)}ms)`;
    const totalLabel = `=== Total ${this.timerName} time`;
    this.log(`[${timestamp}] ${timeInfo} ${totalLabel}`);
  }

  private log(message: string): void {
    const logEntry = `${message}\n`;
    fs.appendFileSync(this.logFile, logEntry);
  }
}

const globalTimers: Record<string, StepTimer> = {};

export function getStepTimer(chainName: string): StepTimer {
  if (!globalTimers[chainName]) {
    const timingLogFile = `../../../logs/highlevel/${chainName}_timing.log`;
    globalTimers[chainName] = new StepTimer(timingLogFile, 'chain initialization');
  }
  return globalTimers[chainName];
}

export function getAllGlobalTimers(): Record<string, StepTimer> {
  return globalTimers;
}

/**
 * Safely removes a timing log file if it exists
 */
export function cleanTimingLog(logFile: string): void {
  if (fs.existsSync(logFile)) {
    fs.unlinkSync(logFile);
  }
}

/**
 * Utility function to read and display timing logs from a file
 */
export function displayTimingLog(logFile: string): void {
  if (fs.existsSync(logFile)) {
    const content = fs.readFileSync(logFile, 'utf8');
    console.log(`Timing log for ${logFile}:`);
    console.log(content);
  } else {
    console.log(`Timing log file not found: ${logFile}`);
  }
}

/**
 * Utility function to get timing statistics from multiple chain logs
 */
export function getTimingStats(logFile: string): { totalTime: number; steps: { name: string; duration: number }[] } {
  if (!fs.existsSync(logFile)) {
    return { totalTime: 0, steps: [] };
  }

  const content = fs.readFileSync(logFile, 'utf8');
  const lines = content.split('\n');
  const steps: { name: string; duration: number }[] = [];
  let totalTime = 0;

  lines.forEach(line => {
    const stepMatch = line.match(/Completed step: (.+?) \((\d+)ms\)/);
    if (stepMatch) {
      const stepName = stepMatch[1];
      const duration = parseInt(stepMatch[2]);
      steps.push({ name: stepName, duration });
      totalTime += duration;
    }
  });

  return { totalTime, steps };
} 

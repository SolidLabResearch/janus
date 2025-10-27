/**
 * Logger utility for Janus RDF Framework
 */

import { LogLevel, ILogger } from '../core/types';

/**
 * Logger configuration
 */
export interface LoggerConfig {
  level: LogLevel;
  enableTimestamp?: boolean;
  enableColors?: boolean;
  context?: string;
}

/**
 * Default logger configuration
 */
const defaultConfig: LoggerConfig = {
  level: LogLevel.Info,
  enableTimestamp: true,
  enableColors: true,
};

/**
 * ANSI color codes for console output
 */
const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  magenta: '\x1b[35m',
  cyan: '\x1b[36m',
  white: '\x1b[37m',
  gray: '\x1b[90m',
};

/**
 * Logger class for structured logging
 */
export class Logger implements ILogger {
  private config: LoggerConfig;
  private context?: string;

  constructor(context?: string, config?: Partial<LoggerConfig>) {
    this.context = context;
    this.config = {
      ...defaultConfig,
      ...config,
      context,
    };
  }

  /**
   * Log a debug message
   */
  debug(message: string, ...args: unknown[]): void {
    if (this.shouldLog(LogLevel.Debug)) {
      this.log(LogLevel.Debug, message, args);
    }
  }

  /**
   * Log an info message
   */
  info(message: string, ...args: unknown[]): void {
    if (this.shouldLog(LogLevel.Info)) {
      this.log(LogLevel.Info, message, args);
    }
  }

  /**
   * Log a warning message
   */
  warn(message: string, ...args: unknown[]): void {
    if (this.shouldLog(LogLevel.Warn)) {
      this.log(LogLevel.Warn, message, args);
    }
  }

  /**
   * Log an error message
   */
  error(message: string, ...args: unknown[]): void {
    if (this.shouldLog(LogLevel.Error)) {
      this.log(LogLevel.Error, message, args);
    }
  }

  /**
   * Set the log level
   */
  setLevel(level: LogLevel): void {
    this.config.level = level;
  }

  /**
   * Get the current log level
   */
  getLevel(): LogLevel {
    return this.config.level;
  }

  /**
   * Set the logger context
   */
  setContext(context: string): void {
    this.context = context;
    this.config.context = context;
  }

  /**
   * Create a child logger with a new context
   */
  child(context: string): Logger {
    const childContext = this.context ? `${this.context}:${context}` : context;
    return new Logger(childContext, this.config);
  }

  /**
   * Check if a log level should be logged
   */
  private shouldLog(level: LogLevel): boolean {
    const levels = [LogLevel.Debug, LogLevel.Info, LogLevel.Warn, LogLevel.Error];
    const currentLevelIndex = levels.indexOf(this.config.level);
    const messageLevelIndex = levels.indexOf(level);
    return messageLevelIndex >= currentLevelIndex;
  }

  /**
   * Internal log method
   */
  private log(level: LogLevel, message: string, args: unknown[]): void {
    const timestamp = this.config.enableTimestamp ? this.getTimestamp() : '';
    const contextStr = this.context ? `[${this.context}]` : '';
    const levelStr = this.formatLevel(level);

    const logMessage = [timestamp, levelStr, contextStr, message].filter(Boolean).join(' ');

    // Use appropriate console method
    switch (level) {
      case LogLevel.Debug:
        console.debug(logMessage, ...args);
        break;
      case LogLevel.Info:
        console.info(logMessage, ...args);
        break;
      case LogLevel.Warn:
        console.warn(logMessage, ...args);
        break;
      case LogLevel.Error:
        console.error(logMessage, ...args);
        break;
    }
  }

  /**
   * Get formatted timestamp
   */
  private getTimestamp(): string {
    const now = new Date();
    const hours = String(now.getHours()).padStart(2, '0');
    const minutes = String(now.getMinutes()).padStart(2, '0');
    const seconds = String(now.getSeconds()).padStart(2, '0');
    const ms = String(now.getMilliseconds()).padStart(3, '0');

    const timestamp = `${hours}:${minutes}:${seconds}.${ms}`;

    if (this.config.enableColors) {
      return `${colors.gray}${timestamp}${colors.reset}`;
    }

    return timestamp;
  }

  /**
   * Format log level with colors
   */
  private formatLevel(level: LogLevel): string {
    const levelMap: Record<LogLevel, { label: string; color: string }> = {
      [LogLevel.Debug]: { label: 'DEBUG', color: colors.cyan },
      [LogLevel.Info]: { label: 'INFO ', color: colors.green },
      [LogLevel.Warn]: { label: 'WARN ', color: colors.yellow },
      [LogLevel.Error]: { label: 'ERROR', color: colors.red },
    };

    const { label, color } = levelMap[level];

    if (this.config.enableColors) {
      return `${color}${label}${colors.reset}`;
    }

    return label;
  }
}

/**
 * Global logger instance
 */
let globalLogger: Logger | null = null;

/**
 * Get or create the global logger instance
 */
export function getLogger(context?: string): Logger {
  if (!globalLogger) {
    globalLogger = new Logger(context);
    return globalLogger;
  }

  if (context) {
    return globalLogger.child(context);
  }

  return globalLogger;
}

/**
 * Configure the global logger
 */
export function configureLogger(config: Partial<LoggerConfig>): void {
  if (!globalLogger) {
    globalLogger = new Logger(undefined, config);
  } else {
    globalLogger.setLevel(config.level || LogLevel.Info);
    if (config.context) {
      globalLogger.setContext(config.context);
    }
  }
}

/**
 * Disable all logging
 */
export function disableLogging(): void {
  if (globalLogger) {
    globalLogger.setLevel(LogLevel.Error);
  }
}

/**
 * Enable verbose logging (debug level)
 */
export function enableVerboseLogging(): void {
  if (globalLogger) {
    globalLogger.setLevel(LogLevel.Debug);
  } else {
    configureLogger({ level: LogLevel.Debug });
  }
}

/**
 * Create a performance logger
 */
export class PerformanceLogger {
  private logger: Logger;
  private startTime: number;
  private context: string;

  constructor(context: string, logger?: Logger) {
    this.context = context;
    this.logger = logger || getLogger('Performance');
    this.startTime = Date.now();
  }

  /**
   * Log the elapsed time and return it
   */
  end(message?: string): number {
    const elapsed = Date.now() - this.startTime;
    const logMessage = message || `${this.context} completed`;
    this.logger.debug(`${logMessage} in ${elapsed}ms`);
    return elapsed;
  }

  /**
   * Log a checkpoint with elapsed time
   */
  checkpoint(label: string): number {
    const elapsed = Date.now() - this.startTime;
    this.logger.debug(`${this.context} - ${label}: ${elapsed}ms`);
    return elapsed;
  }
}

/**
 * Measure execution time of a function
 */
export async function measureTime<T>(
  fn: () => Promise<T>,
  label: string,
  logger?: Logger
): Promise<{ result: T; duration: number }> {
  const perf = new PerformanceLogger(label, logger);
  const result = await fn();
  const duration = perf.end();
  return { result, duration };
}

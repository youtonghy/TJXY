/**
 * Logger utility for HeroUI Core
 * Provides formatted console output with levels and prefixes
 */
interface LoggerOptions {
    enabled?: boolean;
    prefix?: string;
}
export declare class Logger {
    private enabled;
    private prefix;
    constructor(options?: LoggerOptions);
    private formatMessage;
    private log;
    info(message: string, ...args: any[]): void;
    success(message: string, ...args: any[]): void;
    warn(message: string, ...args: any[]): void;
    error(message: string, ...args: any[]): void;
    debug(message: string, ...args: any[]): void;
    divider(char?: string, length?: number): void;
    newline(): void;
}
export {};

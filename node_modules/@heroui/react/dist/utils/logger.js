/* eslint-disable no-console */
/**
 * Logger utility for HeroUI Core
 * Provides formatted console output with levels and prefixes
 */

const colors = {
  blue: "\x1b[34m",
  green: "\x1b[32m",
  magenta: "\x1b[35m",
  red: "\x1b[31m",
  reset: "\x1b[0m",
  yellow: "\x1b[33m"
};
const levelColors = {
  debug: colors.magenta,
  error: colors.red,
  info: colors.blue,
  success: colors.green,
  warn: colors.yellow
};
const levelEmojis = {
  debug: "🔍",
  error: "❌",
  info: "ℹ️",
  success: "✅",
  warn: "⚠️"
};
class Logger {
  constructor(options = {}) {
    this.enabled = options.enabled ?? true;
    this.prefix = options.prefix ?? "HeroUI";
  }
  formatMessage(level, message) {
    const color = levelColors[level];
    const emoji = levelEmojis[level];
    return `${color}[${this.prefix}]${colors.reset} ${emoji}  ${message}`;
  }
  log(level, message, ...args) {
    if (!this.enabled) return;
    const formattedMessage = this.formatMessage(level, message);
    switch (level) {
      case "error":
        console.error(formattedMessage, ...args);
        break;
      case "warn":
        console.warn(formattedMessage, ...args);
        break;
      default:
        console.log(formattedMessage, ...args);
    }
  }
  info(message, ...args) {
    this.log("info", message, ...args);
  }
  success(message, ...args) {
    this.log("success", message, ...args);
  }
  warn(message, ...args) {
    this.log("warn", message, ...args);
  }
  error(message, ...args) {
    this.log("error", message, ...args);
  }
  debug(message, ...args) {
    this.log("debug", message, ...args);
  }
  divider(char = "=", length = 80) {
    if (!this.enabled) return;
    console.log(char.repeat(length));
  }
  newline() {
    if (!this.enabled) return;
    console.log();
  }
}

export { Logger };

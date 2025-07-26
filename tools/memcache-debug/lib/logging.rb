require 'logger'

# Simple colorized logger for debugging output
class ColorLogger < Logger
  COLORS = {
    'DEBUG' => "\e[36m", # Cyan
    'INFO'  => "\e[32m", # Green
    'WARN'  => "\e[33m", # Yellow
    'ERROR' => "\e[31m", # Red
    'FATAL' => "\e[35m", # Magenta
    'RESET' => "\e[0m",      # Reset
    'DARK_GRAY' => "\e[90m"  # Dark gray
  }.freeze

  def initialize(logdev, shift_age = 0, shift_size = 1048576, level: Logger::INFO, colorize: true)
    super(logdev, shift_age, shift_size)
    @level = level
    @colorize = colorize && (logdev == STDOUT || logdev == STDERR) && STDOUT.tty?

    self.formatter = proc do |severity, datetime, progname, msg|
      timestamp = datetime.strftime("%Y-%m-%d %H:%M:%S.%3N")

      if @colorize
        color = COLORS[severity] || COLORS['RESET']
        reset = COLORS['RESET']
        "#{COLORS['DARK_GRAY']}#{timestamp}#{reset} #{color}[#{severity}]#{reset} #{msg}\n"
      else
        "#{timestamp} [#{severity}] #{msg}\n"
      end
    end
  end
end

# Logging functionality for requests and operations
module Logging
  def setup_logger(options)
    log_level = if options[:quiet]
                  Logger::WARN
                elsif options[:verbose]
                  Logger::DEBUG
                else
                  Logger::INFO
                end

    @logger = ColorLogger.new(STDOUT, level: log_level, colorize: !options[:no_color])
  end

  def log_startup_info
    server_name = @name ? " (#{@name})" : ""
    @logger.info "Debug Memcached Server starting on #{@host}:#{@port}#{server_name}"
    @logger.info "Features:"
    @logger.info "  - Request logging: #{@log_requests ? 'enabled' : 'disabled'}"
    @logger.info "  - Artificial delay: #{@delay_ms}ms"
    @logger.info "  - Failure rate: #{(@failure_rate * 100).round(1)}%"
    @logger.info "  - Verbose mode: #{@verbose ? 'enabled' : 'disabled'}"
    @logger.info ""
  end

  def log_request(message, client_info = nil, level = :info)
    return unless @log_requests

    prefix = client_info ? "[#{client_info}] " : ""
    @logger.send(level, "#{prefix}#{message}")
  end

  def log_operation(message, level = :info)
    return unless @log_requests

    @logger.send(level, message)
  end
end
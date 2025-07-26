#!/usr/bin/env ruby

require 'socket'
require 'optparse'
require 'json'
require 'time'
require 'yaml'

# Load our modules
require_relative 'lib/logging'
require_relative 'lib/server_utils'
require_relative 'lib/protocol'

class DebugMemcachedServer
  include Logging
  include ServerUtils::StorageStats
  include ServerUtils::Simulation
  include Protocol::Ascii
  include Protocol::Meta
  include Protocol::Binary

  DEFAULT_PORT = 11212
  DEFAULT_HOST = '127.0.0.1'

  def initialize(options = {})
    @name = options[:name]
    @host = options[:host] || DEFAULT_HOST
    @port = options[:port] || DEFAULT_PORT
    @delay_ms = options[:delay_ms] || 0
    @failure_rate = options[:failure_rate] || 0.0
    @log_requests = options[:log_requests] != false
    @verbose = options[:verbose] || false
    @quiet = options[:quiet] || false
    @no_color = options[:no_color] || false

    # Initialize modules
    setup_logger(options)
    initialize_storage_and_stats

    log_startup_info
  end

  def start
    server = TCPServer.new(@host, @port)
    @logger.info "Server listening on #{@host}:#{@port}"

    trap('INT') {
      @logger.info "\nShutting down server..."
      server.close
      print_stats
      exit 0
    }

    loop do
      Thread.start(server.accept) do |client|
        handle_client(client)
      end
    end
  rescue => e
    @logger.error "Server error: #{e.message}"
  ensure
    server&.close
  end

  private

  def handle_client(client)
    @stats[:total_connections] += 1
    client_info = "#{client.peeraddr[3]}:#{client.peeraddr[1]}"
    log_request("New connection", client_info, :info)

    begin
      # Peek at the first byte to determine the protocol
      first_byte = client.recv(1, Socket::MSG_PEEK)

      if first_byte.bytes.first == 0x80 # Binary protocol
        process_binary_command(client)
      else # ASCII protocol
        while line = client.gets
          line = line.strip
          next if line.empty?

          @stats[:total_commands] += 1

          # Handle simulation (delay/failure)
          simulation_result = simulate_command_processing(line, client_info)
          if simulation_result == :failure
            client.close
            return
          end

          log_request("RX: #{line}", client_info, :debug)

          response = process_ascii_command(line, client)
          if response
            log_request("TX: #{response.strip}", client_info, :debug) if @verbose
            client.write(response)
          end
        end
      end
    rescue => e
      log_request("Client error: #{e.message}", client_info, :error)
    ensure
      client.close rescue nil
      log_request("Connection closed", client_info, :debug)
    end
  end

  def process_ascii_command(line, client)
    parts = line.split(' ')
    command = parts[0].downcase

    case command
    when 'get', 'gets'
      handle_get(parts[1..-1], command)
    when 'mg'
      handle_meta_get(parts)
    when 'ms'
      handle_meta_set(parts, client)
    when 'set'
      handle_set(parts, client)
    when 'add'
      handle_add(parts, client)
    when 'replace'
      handle_replace(parts, client)
    when 'delete'
      handle_delete(parts[1])
    when 'stats'
      handle_stats
    when 'version'
      "VERSION 1.0.0-debug\r\n"
    when 'quit'
      client.close
      nil
    when 'flush_all'
      handle_flush_all
    else
      log_operation("Unknown command: #{command}", :error)
      "ERROR\r\n"
    end
  end
end

class ServerRunner
  def initialize(config_path, global_options = {})
    @config = YAML.load_file(config_path)
    @threads = []
    @global_options = global_options
  end

  def run
    @config['servers'].each do |server_options|
      # Merge global options with server-specific options (server options take precedence)
      merged_options = @global_options.merge(server_options.transform_keys(&:to_sym))

      @threads << Thread.new do
        DebugMemcachedServer.new(merged_options).start
      end
    end

    @threads.each(&:join)
  end
end

# Command line interface
def main
  options = {}

  OptionParser.new do |opts|
    opts.banner = "Usage: #{$0} [options]"

    opts.on("-c", "--config FILE", "Load server configurations from a YAML file") do |file|
      options[:config] = file
    end

    opts.on("-p", "--port PORT", Integer, "Port to listen on (default: #{DebugMemcachedServer::DEFAULT_PORT})") do |port|
      options[:port] = port
    end

    opts.on("-h", "--host HOST", "Host to bind to (default: #{DebugMemcachedServer::DEFAULT_HOST})") do |host|
      options[:host] = host
    end

    opts.on("-d", "--delay MILLISECONDS", Integer, "Artificial delay in milliseconds (default: 0)") do |delay|
      options[:delay_ms] = delay
    end

    opts.on("-f", "--failure-rate RATE", Float, "Failure simulation rate 0.0-1.0 (default: 0.0)") do |rate|
      if rate < 0.0 || rate > 1.0
        STDERR.puts "Failure rate must be between 0.0 and 1.0"
        exit 1
      end
      options[:failure_rate] = rate
    end

    opts.on("-q", "--quiet", "Quiet mode (errors only)") do
      options[:quiet] = true
      options[:log_requests] = false
    end

    opts.on("-v", "--verbose", "Enable verbose output (debug level)") do
      options[:verbose] = true
    end

    opts.on("--no-color", "Disable colored output") do
      options[:no_color] = true
    end

    opts.on("--help", "Show this help") do
      puts opts
      exit 0
    end
  rescue OptionParser::InvalidOption => e
    STDERR.puts "Error: #{e.message}"
    STDERR.puts "Use --help for usage information"
    exit 1
  end.parse!

  # Configuration file and some command-line flags are mutually exclusive
  server_specific_options = [:port, :host, :delay_ms, :failure_rate]
  if options[:config] && options.keys.any? { |k| server_specific_options.include?(k) }
    conflicting_options = options.keys.select { |k| server_specific_options.include?(k) }
    STDERR.puts "Error: The --config option cannot be used with server-specific flags: #{conflicting_options.map { |o| "--#{o.to_s.gsub('_', '-')}" }.join(', ')}"
    STDERR.puts "Please define these configurations in your YAML file."
    STDERR.puts "Global options like --verbose, --quiet, and --no-color can still be used with --config."
    exit 1
  end

  if options[:config]
    unless File.exist?(options[:config])
      STDERR.puts "Error: Configuration file '#{options[:config]}' not found."
      exit 1
    end

    begin
      ServerRunner.new(options[:config], options).run
    rescue => e
      STDERR.puts "Error loading configuration: #{e.message}"
      exit 1
    end
  else
    server = DebugMemcachedServer.new(options)
    server.start
  end
end

if __FILE__ == $0
  main
end
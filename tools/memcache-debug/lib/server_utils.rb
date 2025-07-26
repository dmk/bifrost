require 'logger'

# Server utility functions
module ServerUtils
  # Storage and statistics tracking
  module StorageStats
    def initialize_storage_and_stats
      @storage = {}
      @stats = {
        total_connections: 0,
        total_commands: 0,
        get_hits: 0,
        get_misses: 0,
        sets: 0,
        deletes: 0,
        start_time: Time.now,
        cas_counter: 1
      }
    end

    def next_cas
      @stats[:cas_counter] += 1
      @stats[:cas_counter]
    end

    def print_stats
      @logger.info "\n=== Server Statistics ==="
      @logger.info "Total connections: #{@stats[:total_connections]}"
      @logger.info "Total commands: #{@stats[:total_commands]}"
      @logger.info "Items in storage: #{@storage.size}"
      @logger.info "GET hits: #{@stats[:get_hits]}"
      @logger.info "GET misses: #{@stats[:get_misses]}"
      @logger.info "SETs: #{@stats[:sets]}"
      @logger.info "DELETEs: #{@stats[:deletes]}"
      uptime = Time.now - @stats[:start_time]
      @logger.info "Uptime: #{uptime.round(2)} seconds"
    end

    def increment_stat(stat_name)
      @stats[stat_name] += 1
    end

    def get_stat(stat_name)
      @stats[stat_name]
    end

    def storage_size
      @storage.size
    end

    def clear_storage
      @storage.clear
    end
  end

  # Delay and failure simulation for testing
  module Simulation
    def should_simulate_failure?
      @failure_rate > 0 && rand < @failure_rate
    end

    def apply_artificial_delay
      return unless @delay_ms > 0

      sleep(@delay_ms / 1000.0)
    end

    def simulate_command_processing(line, client_info)
      # Apply artificial delay
      apply_artificial_delay

      # Check for simulated failures
      if should_simulate_failure?
        log_request("SIMULATING FAILURE for command: #{line}", client_info, :warn)
        return :failure
      end

      :success
    end
  end
end
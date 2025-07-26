# Memcached ASCII protocol command handlers
module Protocol
  module Ascii
    def handle_get(keys, command = 'get')
      response = ""
      keys.each do |key|
        if @storage.key?(key)
          item = @storage[key]
          cas_str = (command == 'gets') ? " #{item[:cas]}" : ""
          response += "VALUE #{key} #{item[:flags]} #{item[:data].bytesize}#{cas_str}\r\n"
          response += "#{item[:data]}\r\n"
          @stats[:get_hits] += 1
          log_operation("GET HIT: #{key}", :info)
        else
          @stats[:get_misses] += 1
          log_operation("GET MISS: #{key}", :debug)
        end
      end
      response += "END\r\n"
      response
    end

    def handle_set(parts, client)
      key, flags, exptime, bytes = parts[1], parts[2].to_i, parts[3].to_i, parts[4].to_i
      data = read_data_from_client(client, bytes)

      @storage[key] = {
        flags: flags,
        exptime: exptime,
        data: data,
        cas: next_cas
      }

      @stats[:sets] += 1
      log_operation("SET: #{key} = #{data.inspect} (#{bytes} bytes)", :info)
      "STORED\r\n"
    end

    def handle_add(parts, client)
      key, flags, exptime, bytes = parts[1], parts[2].to_i, parts[3].to_i, parts[4].to_i
      data = read_data_from_client(client, bytes)

      if @storage.key?(key)
        log_operation("ADD FAILED: #{key} already exists", :warn)
        "NOT_STORED\r\n"
      else
        @storage[key] = {
          flags: flags,
          exptime: exptime,
          data: data,
          cas: next_cas
        }
        log_operation("ADD: #{key} = #{data.inspect}", :info)
        "STORED\r\n"
      end
    end

    def handle_replace(parts, client)
      key, flags, exptime, bytes = parts[1], parts[2].to_i, parts[3].to_i, parts[4].to_i
      data = read_data_from_client(client, bytes)

      if @storage.key?(key)
        @storage[key] = {
          flags: flags,
          exptime: exptime,
          data: data,
          cas: next_cas
        }
        log_operation("REPLACE: #{key} = #{data.inspect}", :info)
        "STORED\r\n"
      else
        log_operation("REPLACE FAILED: #{key} not found", :warn)
        "NOT_STORED\r\n"
      end
    end

    def handle_delete(key)
      if @storage.delete(key)
        @stats[:deletes] += 1
        log_operation("DELETE: #{key}", :info)
        "DELETED\r\n"
      else
        log_operation("DELETE FAILED: #{key} not found", :warn)
        "NOT_FOUND\r\n"
      end
    end

    def handle_stats
      uptime = Time.now - @stats[:start_time]

      stats_data = {
        total_connections: @stats[:total_connections],
        total_items: @storage.size,
        cmd_get: @stats[:get_hits] + @stats[:get_misses],
        cmd_set: @stats[:sets],
        get_hits: @stats[:get_hits],
        get_misses: @stats[:get_misses],
        delete_hits: @stats[:deletes],
        uptime: uptime.to_i,
        version: "1.0.0-debug",
        delay_ms: @delay_ms,
        failure_rate: @failure_rate,
      }

      stats_response = stats_data.map { |k, v| "STAT #{k} #{v}\r\n" }.join
      stats_response += "END\r\n"

      log_operation("STATS requested", :info)
      stats_response
    end

    def handle_flush_all
      @storage.clear
      log_operation("FLUSH_ALL: cleared all data", :warn)
      "OK\r\n"
    end

    private

    def read_data_from_client(client, bytes)
      data = client.read(bytes)
      client.gets # consume the trailing \r\n
      data
    end
  end
end
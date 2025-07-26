# Memcached meta protocol command handlers
module Protocol
  module Meta
    def handle_meta_get(parts)
      key = parts[1]
      if @storage.key?(key)
        item = @storage[key]
        response = "HD #{key} T#{item[:exptime]} F#{item[:flags]} c#{item[:cas]} s#{item[:data].bytesize}\r\n"
        response += "#{item[:data]}\r\n"
        @stats[:get_hits] += 1
        log_operation("META GET HIT: #{key}", :info)
      else
        response = "EN\r\n"
        @stats[:get_misses] += 1
        log_operation("META GET MISS: #{key}", :debug)
      end
      response
    end

    def handle_meta_set(parts, client)
      key = parts[1]
      bytes = parts[2].to_i
      flags = 0
      exptime = 0

      # Parse flags from remaining parts
      parts[3..-1].each do |part|
        case part[0]
        when 'T'
          exptime = part[1..-1].to_i
        when 'F'
          flags = part[1..-1].to_i
        end
      end

      data = read_data_from_client(client, bytes)

      @storage[key] = {
        flags: flags,
        exptime: exptime,
        data: data,
        cas: next_cas
      }

      @stats[:sets] += 1
      log_operation("META SET: #{key} = #{data.inspect} (#{bytes} bytes)", :info)
      "HD\r\n" # HD means "header data" in meta protocol, indicating success
    end

    private

    def read_data_from_client(client, bytes)
      data = client.read(bytes)
      client.gets # consume the trailing \r\n
      data
    end
  end
end
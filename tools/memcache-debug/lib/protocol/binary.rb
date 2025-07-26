# Memcached binary protocol command handlers
module Protocol
  module Binary
    def process_binary_command(client)
      # Read and parse the 24-byte header
      header = client.read(24)
      return unless header && header.bytesize == 24

      magic, opcode, key_length, extras_length, data_type, status, total_body_length, opaque, cas = header.unpack('C C n C C n N N Q>')

      # We only handle request packets
      return unless magic == 0x80

      # Read the rest of the packet
      body = client.read(total_body_length)
      return unless body && body.bytesize == total_body_length

      extras = body[0...extras_length]
      key = body[extras_length...(extras_length + key_length)]
      value = body[(extras_length + key_length)...total_body_length]

      # Process the command based on the opcode
      case opcode
      when 0x00 # Get
        handle_binary_get(client, key, opaque, cas)
      when 0x01 # Set
        handle_binary_set(client, key, value, extras, opaque, cas)
      else
        # Respond with "Unknown command"
        response_header = [0x81, opcode, 0, 0, 0, 0x0081, 0, opaque, cas].pack('C C n C C n N N Q>')
        client.write(response_header)
      end
    end

    private

    def handle_binary_get(client, key, opaque, cas)
      if @storage.key?(key)
        item = @storage[key]
        extras = [item[:flags]].pack('N')
        value = item[:data]
        response_header = [0x81, 0x00, 0, extras.bytesize, 0, 0, extras.bytesize + value.bytesize, opaque, cas].pack('C C n C C n N N Q>')
        client.write(response_header)
        client.write(extras)
        client.write(value)
        @stats[:get_hits] += 1
        log_operation("BINARY GET HIT: #{key}", :info)
      else
        response_header = [0x81, 0x00, 0, 0, 0, 1, 0, opaque, cas].pack('C C n C C n N N Q>')
        client.write(response_header)
        @stats[:get_misses] += 1
        log_operation("BINARY GET MISS: #{key}", :debug)
      end
    end

    def handle_binary_set(client, key, value, extras, opaque, cas)
      flags, exptime = extras.unpack('N N')
      @storage[key] = {
        flags: flags,
        exptime: exptime,
        data: value,
        cas: next_cas  # Use our own CAS counter for consistency
      }
      @stats[:sets] += 1
      log_operation("BINARY SET: #{key} = #{value.inspect} (#{value.bytesize} bytes)", :info)

      # Respond with success
      response_header = [0x81, 0x01, 0, 0, 0, 0, 0, opaque, cas].pack('C C n C C n N N Q>')
      client.write(response_header)
    end
  end
end
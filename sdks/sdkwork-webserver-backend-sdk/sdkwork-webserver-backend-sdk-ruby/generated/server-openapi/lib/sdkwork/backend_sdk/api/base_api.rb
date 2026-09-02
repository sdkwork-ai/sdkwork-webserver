cgi
json

module Sdkwork
  module BackendSdk
    module Api
      class BaseApi
        def initialize(client)
          @client = client
        end

        private

        def interpolate_path(path, path_params = {})
          path_params.each do |name, value|
            path = path.gsub("{#{name}}", CGI.escape(value.to_s))
          end

          path
        end

        def serialize_path_parameter(value, spec)
          return '' if value.nil?

          style = spec.style.to_s.empty? ? 'simple' : spec.style
          return serialize_path_array(spec.name, value, style, spec.explode) if value.is_a?(Array)
          return serialize_path_object(spec.name, value, style, spec.explode) if value.is_a?(Hash)

          "#{path_primitive_prefix(spec.name, style)}#{CGI.escape(value.to_s)}"
        end

        def serialize_path_array(name, values, style, explode)
          serialized = values.compact.map { |item| CGI.escape(item.to_s) }
          return path_prefix(name, style) if serialized.empty?
          if style == 'matrix'
            return serialized.map { |item| ";#{name}=#{item}" }.join if explode

            return ";#{name}=#{serialized.join(',')}"
          end

          "#{path_prefix(name, style)}#{serialized.join(explode ? '.' : ',')}"
        end

        def serialize_path_object(name, values, style, explode)
          entries = []
          exploded = []
          values.each do |key, value|
            next if value.nil?

            escaped_key = CGI.escape(key.to_s)
            escaped_value = CGI.escape(value.to_s)
            if explode
              exploded << (style == 'matrix' ? ";#{escaped_key}=#{escaped_value}" : "#{escaped_key}=#{escaped_value}")
            else
              entries << escaped_key
              entries << escaped_value
            end
          end

          if style == 'matrix'
            return exploded.join if explode

            return ";#{name}=#{entries.join(',')}"
          end

          return "#{path_prefix(name, style)}#{exploded.join(style == 'label' ? '.' : ',')}" if explode

          "#{path_prefix(name, style)}#{entries.join(',')}"
        end

        def path_prefix(name, style)
          return '.' if style == 'label'
          return ";#{name}" if style == 'matrix'

          ''
        end

        def path_primitive_prefix(name, style)
          style == 'matrix' ? ";#{name}=" : path_prefix(name, style)
        end

        def append_query_string(path, raw_query_string)
          query = raw_query_string.to_s.sub(/^?+/, '')
          return path if query.empty?

          path.include?('?') ? "#{path}&#{query}" : "#{path}?#{query}"
        end

        def build_query_string(parameters)
          parameters.flat_map { |parameter| serialize_query_parameter(parameter) }.compact.join('&')
        end

        def serialize_query_parameter(parameter)
          return [] if parameter.value.nil?

          if parameter.content_type && !parameter.content_type.empty?
            return ["#{CGI.escape(parameter.name)}=#{encode_query_value(JSON.generate(parameter.value), parameter.allow_reserved)}"]
          end

          style = parameter.style.to_s.empty? ? 'form' : parameter.style
          value = parameter.value
          return serialize_deep_object_parameter(parameter.name, value, parameter.allow_reserved) if style == 'deepObject' && value.is_a?(Hash)
          return serialize_array_parameter(parameter.name, value, style, parameter.explode, parameter.allow_reserved) if value.is_a?(Array)
          return serialize_object_parameter(parameter.name, value, style, parameter.explode, parameter.allow_reserved) if value.is_a?(Hash)

          ["#{CGI.escape(parameter.name)}=#{encode_query_value(value.to_s, parameter.allow_reserved)}"]
        end

        def serialize_array_parameter(name, values, style, explode, allow_reserved)
          serialized = values.compact.map(&:to_s)
          return [] if serialized.empty?
          return serialized.map { |item| "#{CGI.escape(name)}=#{encode_query_value(item, allow_reserved)}" } if style == 'form' && explode

          ["#{CGI.escape(name)}=#{encode_query_value(serialized.join(','), allow_reserved)}"]
        end

        def serialize_object_parameter(name, values, style, explode, allow_reserved)
          serialized = []
          pairs = []
          values.each do |key, value|
            next if value.nil?
            if style == 'form' && explode
              pairs << "#{CGI.escape(key.to_s)}=#{encode_query_value(value.to_s, allow_reserved)}"
            else
              serialized << key.to_s
              serialized << value.to_s
            end
          end
          return pairs if style == 'form' && explode
          return [] if serialized.empty?

          ["#{CGI.escape(name)}=#{encode_query_value(serialized.join(','), allow_reserved)}"]
        end

        def serialize_deep_object_parameter(name, values, allow_reserved)
          values.filter_map do |key, value|
            next if value.nil?

            "#{CGI.escape("#{name}[#{key}]")}=#{encode_query_value(value.to_s, allow_reserved)}"
          end
        end

        def encode_query_value(value, allow_reserved)
          encoded = CGI.escape(value)
          return encoded unless allow_reserved

          {
            '%3A' => ':', '%2F' => '/', '%3F' => '?', '%23' => '#',
            '%5B' => '[', '%5D' => ']', '%40' => '@', '%21' => '!',
            '%24' => '$', '%26' => '&', '%27' => "'", '%28' => '(',
            '%29' => ')', '%2A' => '*', '%2B' => '+', '%2C' => ',',
            '%3B' => ';', '%3D' => '='
          }.each { |escaped, reserved| encoded = encoded.gsub(escaped, reserved) }
          encoded
        end
      end

      QueryParameterSpec = Struct.new(:name, :value, :style, :explode, :allow_reserved, :content_type, keyword_init: false)
      PathParameterSpec = Struct.new(:name, :style, :explode, keyword_init: false)
      HeaderParameterSpec = Struct.new(:value, :style, :explode, :content_type, keyword_init: false)
    end
  end
end

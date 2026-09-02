require 'faraday'
require 'json'

module Sdkwork
  module BackendSdk
    module Http
      class Client
        attr_reader :connection, :headers

        def initialize(config)
          @config = config
          @headers = (config.headers || {}).dup
          @api_key = nil
          @auth_token = nil
          @access_token = nil
          connection_options = normalize_connection_options(config.connection_options)
          test_stubs = connection_options.delete(:test_stubs)
          adapter = connection_options.delete(:adapter)
          adapter_options = connection_options.delete(:adapter_options)
          @connection = Faraday.new({ url: config.base_url }.merge(connection_options)) do |faraday|
            faraday.options.timeout = config.timeout
            if test_stubs
              faraday.adapter :test, test_stubs
            elsif adapter_options
              faraday.adapter adapter || Faraday.default_adapter, adapter_options
            else
              faraday.adapter adapter || Faraday.default_adapter
            end
          end
        end

        def set_api_key(api_key)
          @api_key = api_key
          @auth_token = nil
          @access_token = nil
          self
        end

        def set_auth_token(token)
          @auth_token = token
          @api_key = nil unless 'X-SDKWork-Agent-Token'.downcase == 'authorization'
          self
        end

        def set_access_token(token)
          @access_token = token
          @api_key = nil unless 'X-SDKWork-Agent-Token'.downcase == 'access-token'
          self
        end

        def set_header(key, value)
          @headers[key] = value
          self
        end

        def request(method, path, query: {}, headers: {}, json: nil, form: nil, multipart: nil, skip_auth: false, access_token_only: false)
          response = @connection.run_request(method.to_sym, path, nil, build_headers(headers, skip_auth: skip_auth, access_token_only: access_token_only)) do |request|
            request.params.update(query) unless query.nil? || query.empty?

            if multipart
              request.body = normalize_multipart(multipart)
              request.headers['Content-Type'] = 'multipart/form-data'
            elsif form
              request.body = form
              request.headers['Content-Type'] = 'application/x-www-form-urlencoded'
            elsif !json.nil?
              request.body = JSON.generate(json)
              request.headers['Content-Type'] = 'application/json'
            end
          end

          parse_response(response)
        rescue Faraday::Error => e
          raise RuntimeError, "SDK request failed: #{e.message}"
        end

        def stream(method, path, query: {}, headers: {}, json: nil, form: nil, multipart: nil, skip_auth: false, access_token_only: false)
          response = @connection.run_request(method.to_sym, path, nil, build_headers({ 'Accept' => 'text/event-stream' }.merge(headers || {}), skip_auth: skip_auth, access_token_only: access_token_only)) do |request|
            request.params.update(query) unless query.nil? || query.empty?

            if multipart
              request.body = normalize_multipart(multipart)
              request.headers['Content-Type'] = 'multipart/form-data'
            elsif form
              request.body = form
              request.headers['Content-Type'] = 'application/x-www-form-urlencoded'
            elsif !json.nil?
              request.body = JSON.generate(json)
              request.headers['Content-Type'] = 'application/json'
            end
          end

          Enumerator.new do |yielder|
            response.body.to_s.split(/\r?\n\r?\n/).each do |raw_event|
              data_lines = raw_event.each_line.filter_map { |line| line.start_with?('data:') ? line.sub(/^data:\s*/, '').strip : nil }
              next if data_lines.empty?

              data = data_lines.join("\n")
              break if data == '[DONE]'

              yielder << JSON.parse(data)
            end
          end
        rescue Faraday::Error => e
          raise RuntimeError, "SDK stream failed: #{e.message}"
        end

        private

        def build_headers(request_headers, skip_auth: false, access_token_only: false)
          auth_headers = {}
          auth_headers['X-SDKWork-Agent-Token'] = @api_key if @api_key && !@api_key.empty?
          auth_headers['Authorization'] = format_bearer(@auth_token) if @auth_token && !@auth_token.empty?
          auth_headers['Access-Token'] = @access_token if @access_token && !@access_token.empty?
          return auth_headers.merge(@headers).merge(request_headers || {}) unless skip_auth || access_token_only

          resolved_headers = strip_credential_headers(request_headers || {})
          if access_token_only
            access_token = @access_token.to_s.strip
            raise RuntimeError, 'access-token-only request requires Access-Token before request dispatch' if access_token.empty?

            resolved_headers['Access-Token'] = access_token
          end
          resolved_headers
        end

        def strip_credential_headers(headers)
          forbidden = %w[
            authorization access-token x-api-key x-tenant-id x-organization-id x-platform x-user-id
            x-sdkwork-tenant-id x-sdkwork-organization-id x-sdkwork-user-id
          ]
          headers.reject { |key, _value| forbidden.include?(key.to_s.downcase) }
        end

        def parse_response(response)
          body = response.body.to_s
          return nil if body.empty?

          JSON.parse(body)
        rescue JSON::ParserError
          body
        end

        def normalize_multipart(payload)
          return [] unless payload.is_a?(Hash)

          payload.map do |name, value|
            {
              name: name.to_s,
              content_type: value.is_a?(Hash) || value.is_a?(Array) ? 'application/json' : nil,
              value: value.is_a?(Hash) || value.is_a?(Array) ? JSON.generate(value) : value,
            }.compact
          end
        end

        def format_bearer(value)
          "Bearer #{value}"
        end

        def normalize_connection_options(options)
          return {} unless options.is_a?(Hash)

          options.each_with_object({}) do |(key, value), normalized|
            normalized[key.respond_to?(:to_sym) ? key.to_sym : key] = value
          end
        end
      end
    end
  end
end

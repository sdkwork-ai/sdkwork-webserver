require_relative 'base_api'
require_relative '../models/webserver_configs_list_response'
require_relative '../models/webserver_configs_retrieve_response'
require_relative '../models/webserver_configs_update_response'
require_relative '../models/webserver_config_write_request'

module Sdkwork
  module BackendSdk
    module Api
      class WebserverConfigApi < BaseApi
          # List the managed Web Server configuration catalog
          def webserver_configs_list()
            path = '/backend/v3/api/webserver_configs'
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::WebserverConfigsListResponse.from_hash(result) : nil
          end

          # Read one managed configuration file
          def webserver_configs_retrieve(config_id)
            path = interpolate_path('/backend/v3/api/webserver_configs/{configId}', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::WebserverConfigsRetrieveResponse.from_hash(result) : nil
          end

          # Validate and atomically overwrite one managed configuration file
          def webserver_configs_update(config_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/webserver_configs/{configId}', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            options[:json] = payload unless payload.nil?
            result = @client.request('PUT', path, **options)
            result.is_a?(Hash) ? Models::WebserverConfigsUpdateResponse.from_hash(result) : nil
          end

        private

        def build_request_headers(headers = {}, cookies = {})
          request_headers = {}
          headers.each do |name, parameter|
            serialized = serialize_parameter_value(parameter)
            request_headers[name.to_s] = serialized unless serialized.nil?
          end

          cookie_header = build_cookie_header(cookies)
          unless cookie_header.empty?
            request_headers['Cookie'] =
              request_headers.key?('Cookie') && !request_headers['Cookie'].empty? ? "#{request_headers['Cookie']}; #{cookie_header}" : cookie_header
          end

          request_headers
        end

        def build_cookie_header(cookies = {})
          cookies.filter_map do |name, parameter|
            serialized = serialize_parameter_value(parameter)
            next if serialized.nil?

            "#{CGI.escape(name.to_s)}=#{CGI.escape(serialized)}"
          end.join('; ')
        end

        def serialize_parameter_value(parameter)
          value = parameter&.value
          return nil if value.nil?
          return JSON.generate(value) if parameter.content_type && !parameter.content_type.empty?
          return value.compact.map(&:to_s).join(',') if value.is_a?(Array)
          if value.is_a?(Hash)
            serialized = []
            value.each do |key, item|
              next if item.nil?
              if parameter.explode
                serialized << "#{key}=#{item}"
              else
                serialized << key.to_s
                serialized << item.to_s
              end
            end
            return serialized.join(',')
          end
          return value.iso8601 if value.respond_to?(:iso8601)

          value.to_s
        end
      end
    end
  end
end

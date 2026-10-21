require_relative 'base_api'
require_relative '../models/create_plugin_request'
require_relative '../models/plugins_create_response201'
require_relative '../models/plugins_list_response'
require_relative '../models/plugins_retrieve_response'
require_relative '../models/plugins_update_response'
require_relative '../models/update_plugin_request'

module Sdkwork
  module BackendSdk
    module Api
      class PluginApi < BaseApi
          # List the caller's plugin catalog
          def plugins_list(page: nil, page_size: nil, status: nil, q: nil, owner_id: nil)
            path = '/backend/v3/api/plugins'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
              QueryParameterSpec.new('q', q, 'form', true, false, nil),
              QueryParameterSpec.new('owner_id', owner_id, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::PluginsListResponse.from_hash(result) : nil
          end

          # Create a plugin catalog record
          def plugins_create(idempotency_key, body: nil)
            path = '/backend/v3/api/plugins'
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
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::PluginsCreateResponse201.from_hash(result) : nil
          end

          # Retrieve one plugin catalog record
          def plugins_retrieve(plugin_id)
            path = interpolate_path('/backend/v3/api/plugins/{pluginId}', pluginId: serialize_path_parameter(plugin_id, PathParameterSpec.new('pluginId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::PluginsRetrieveResponse.from_hash(result) : nil
          end

          # Update one plugin catalog record
          def plugins_update(plugin_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/plugins/{pluginId}', pluginId: serialize_path_parameter(plugin_id, PathParameterSpec.new('pluginId', 'simple', false)))
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
            result = @client.request('PATCH', path, **options)
            result.is_a?(Hash) ? Models::PluginsUpdateResponse.from_hash(result) : nil
          end

          # Delete one plugin catalog record
          def plugins_delete(plugin_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/plugins/{pluginId}', pluginId: serialize_path_parameter(plugin_id, PathParameterSpec.new('pluginId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            result = @client.request('DELETE', path, **options)
            result
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

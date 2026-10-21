require_relative 'base_api'
require_relative '../models/create_tunnel_machine_route_request'
require_relative '../models/set_tunnel_machine_enabled_request'
require_relative '../models/tunnel_machines_enabled_update_response'
require_relative '../models/tunnel_machines_list_response'
require_relative '../models/tunnel_machines_retrieve_response'
require_relative '../models/tunnel_machines_routes_create_response201'

module Sdkwork
  module BackendSdk
    module Api
      class TunnelApi < BaseApi
          # List the tunnel connector machines this gateway manages
          def machines_list(page: nil, page_size: nil)
            path = '/backend/v3/api/tunnel_machines'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::TunnelMachinesListResponse.from_hash(result) : nil
          end

          # Retrieve one managed tunnel machine
          def machines_retrieve(device_id)
            path = interpolate_path('/backend/v3/api/tunnel_machines/{deviceId}', deviceId: serialize_path_parameter(device_id, PathParameterSpec.new('deviceId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::TunnelMachinesRetrieveResponse.from_hash(result) : nil
          end

          # Switch one tunnel machine on or off
          def machines_enabled_update(device_id, body: nil)
            path = interpolate_path('/backend/v3/api/tunnel_machines/{deviceId}/enabled', deviceId: serialize_path_parameter(device_id, PathParameterSpec.new('deviceId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            options = {}
            options[:json] = payload unless payload.nil?
            result = @client.request('PUT', path, **options)
            result.is_a?(Hash) ? Models::TunnelMachinesEnabledUpdateResponse.from_hash(result) : nil
          end

          # Declare a route for one tunnel machine
          def machines_routes_create(device_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/tunnel_machines/{deviceId}/routes', deviceId: serialize_path_parameter(device_id, PathParameterSpec.new('deviceId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::TunnelMachinesRoutesCreateResponse201.from_hash(result) : nil
          end

          # Remove one route from a tunnel machine
          def machines_routes_delete(device_id, route_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/tunnel_machines/{deviceId}/routes/{routeId}', deviceId: serialize_path_parameter(device_id, PathParameterSpec.new('deviceId', 'simple', false)), routeId: serialize_path_parameter(route_id, PathParameterSpec.new('routeId', 'simple', false)))
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

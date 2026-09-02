require_relative 'base_api'
require_relative '../models/configs_create_response201'
require_relative '../models/configs_deploy_response'
require_relative '../models/configs_list_response'
require_relative '../models/configs_retrieve_response'
require_relative '../models/configs_update_response'
require_relative '../models/configs_validate_response'
require_relative '../models/create_nginx_config_request'
require_relative '../models/reload_response'
require_relative '../models/status_retrieve_response'
require_relative '../models/update_nginx_config_request'

module Sdkwork
  module BackendSdk
    module Api
      class NginxApi < BaseApi
          # List Nginx configurations
          def configs_list(page: nil, page_size: nil, site_id: nil, config_type: nil, is_active: nil)
            path = '/backend/v3/api/nginx/configs'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('site_id', site_id, 'form', true, false, nil),
              QueryParameterSpec.new('config_type', config_type, 'form', true, false, nil),
              QueryParameterSpec.new('is_active', is_active, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ConfigsListResponse.from_hash(result) : nil
          end

          # Create an Nginx configuration
          def configs_create(idempotency_key, body: nil)
            path = '/backend/v3/api/nginx/configs'
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
            result.is_a?(Hash) ? Models::ConfigsCreateResponse201.from_hash(result) : nil
          end

          # Retrieve an Nginx configuration
          def configs_retrieve(config_id)
            path = interpolate_path('/backend/v3/api/nginx/etc/{configId}', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ConfigsRetrieveResponse.from_hash(result) : nil
          end

          # Update an Nginx configuration
          def configs_update(config_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/nginx/etc/{configId}', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ConfigsUpdateResponse.from_hash(result) : nil
          end

          # Validate an Nginx configuration
          def configs_validate(config_id)
            path = interpolate_path('/backend/v3/api/nginx/etc/{configId}/validate', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ConfigsValidateResponse.from_hash(result) : nil
          end

          # Deploy an Nginx configuration
          def configs_deploy(config_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/nginx/etc/{configId}/deploy', configId: serialize_path_parameter(config_id, PathParameterSpec.new('configId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ConfigsDeployResponse.from_hash(result) : nil
          end

          # Reload Nginx
          def reload(idempotency_key)
            path = '/backend/v3/api/nginx/reload'
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ReloadResponse.from_hash(result) : nil
          end

          # Retrieve Nginx status
          def status_retrieve()
            path = '/backend/v3/api/nginx/status'
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::StatusRetrieveResponse.from_hash(result) : nil
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

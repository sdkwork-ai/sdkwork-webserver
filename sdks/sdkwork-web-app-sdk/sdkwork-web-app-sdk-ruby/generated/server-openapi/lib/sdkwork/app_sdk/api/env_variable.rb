require_relative 'base_api'
require_relative '../models/applications_env_variables_create_response201'
require_relative '../models/applications_env_variables_list_response'
require_relative '../models/applications_env_variables_update_response'
require_relative '../models/create_env_variable_request'
require_relative '../models/update_env_variable_request'

module Sdkwork
  module AppSdk
    module Api
      class EnvVariableApi < BaseApi
          # 获取环境变量列表
          def applications_env_variables_list(application_id, environment: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/env_variables', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('environment', environment, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsEnvVariablesListResponse.from_hash(result) : nil
          end

          # 创建环境变量
          def applications_env_variables_create(application_id, idempotency_key, body: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/env_variables', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsEnvVariablesCreateResponse201.from_hash(result) : nil
          end

          # 轮换环境变量值
          def applications_env_variables_update(application_id, variable_id, idempotency_key, body: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/env_variables/{variableId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), variableId: serialize_path_parameter(variable_id, PathParameterSpec.new('variableId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsEnvVariablesUpdateResponse.from_hash(result) : nil
          end

          # 删除环境变量
          def applications_env_variables_delete(application_id, variable_id, idempotency_key)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/env_variables/{variableId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), variableId: serialize_path_parameter(variable_id, PathParameterSpec.new('variableId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            @client.request('DELETE', path, **options)
            nil
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

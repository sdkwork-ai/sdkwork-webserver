require_relative 'base_api'
require_relative '../models/applications_activate_response'
require_relative '../models/applications_create_response201'
require_relative '../models/applications_list_response'
require_relative '../models/applications_pause_response'
require_relative '../models/applications_platform_targets_create_response201'
require_relative '../models/applications_platform_targets_list_response'
require_relative '../models/applications_platform_targets_retrieve_response'
require_relative '../models/applications_retrieve_response'
require_relative '../models/applications_update_response'
require_relative '../models/create_application_request'
require_relative '../models/create_platform_target_request'
require_relative '../models/update_application_request'

module Sdkwork
  module AppSdk
    module Api
      class ApplicationApi < BaseApi
          # 获取应用列表
          def applications_list(page: nil, page_size: nil, status: nil, application_type: nil, site_type: nil, keyword: nil)
            path = '/app/v3/api/applications'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
              QueryParameterSpec.new('application_type', application_type, 'form', true, false, nil),
              QueryParameterSpec.new('site_type', site_type, 'form', true, false, nil),
              QueryParameterSpec.new('keyword', keyword, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsListResponse.from_hash(result) : nil
          end

          # 创建应用
          def applications_create(idempotency_key, body: nil)
            path = '/app/v3/api/applications'
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
            result.is_a?(Hash) ? Models::ApplicationsCreateResponse201.from_hash(result) : nil
          end

          # 获取应用详情
          def applications_retrieve(application_id)
            path = interpolate_path('/app/v3/api/applications/{applicationId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsRetrieveResponse.from_hash(result) : nil
          end

          # 更新应用
          def applications_update(application_id, idempotency_key, body: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsUpdateResponse.from_hash(result) : nil
          end

          # 删除应用
          def applications_delete(application_id)
            path = interpolate_path('/app/v3/api/applications/{applicationId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            options = {}

            result = @client.request('DELETE', path, **options)
            result
          end

          # 激活应用
          def applications_activate(application_id)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/activate', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsActivateResponse.from_hash(result) : nil
          end

          # 暂停应用
          def applications_pause(application_id)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/pause', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            options = {}

            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsPauseResponse.from_hash(result) : nil
          end

          # 获取应用平台目标列表
          def applications_platform_targets_list(application_id, page: nil, page_size: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/platform_targets', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsPlatformTargetsListResponse.from_hash(result) : nil
          end

          # 创建应用平台目标
          def applications_platform_targets_create(application_id, idempotency_key, body: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/platform_targets', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsPlatformTargetsCreateResponse201.from_hash(result) : nil
          end

          # 获取应用平台目标详情
          def applications_platform_targets_retrieve(application_id, platform_target_id)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/platform_targets/{platformTargetId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), platformTargetId: serialize_path_parameter(platform_target_id, PathParameterSpec.new('platformTargetId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsPlatformTargetsRetrieveResponse.from_hash(result) : nil
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

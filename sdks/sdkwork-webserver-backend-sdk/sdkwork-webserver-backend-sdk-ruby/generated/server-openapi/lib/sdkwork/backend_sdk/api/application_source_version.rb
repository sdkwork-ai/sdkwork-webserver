require_relative 'base_api'
require_relative '../models/applications_source_versions_create_response201'
require_relative '../models/applications_source_versions_git_import_create_response201'
require_relative '../models/applications_source_versions_list_response'
require_relative '../models/applications_source_versions_retrieve_response'
require_relative '../models/create_application_source_version_request'
require_relative '../models/import_application_git_source_version_request'

module Sdkwork
  module BackendSdk
    module Api
      class ApplicationSourceVersionApi < BaseApi
          # List immutable application source versions
          def applications_source_versions_list(application_id, page_size: nil, cursor: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/source_versions', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsSourceVersionsListResponse.from_hash(result) : nil
          end

          # Register an immutable Drive-backed application source version
          def applications_source_versions_create(application_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/source_versions', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsSourceVersionsCreateResponse201.from_hash(result) : nil
          end

          # Import an immutable application source version from a public Git repository
          def applications_source_versions_git_import_create(application_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/source_versions/git_import', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsSourceVersionsGitImportCreateResponse201.from_hash(result) : nil
          end

          # Retrieve an application source version
          def applications_source_versions_retrieve(application_id, source_version_id)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/source_versions/{sourceVersionId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), sourceVersionId: serialize_path_parameter(source_version_id, PathParameterSpec.new('sourceVersionId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsSourceVersionsRetrieveResponse.from_hash(result) : nil
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

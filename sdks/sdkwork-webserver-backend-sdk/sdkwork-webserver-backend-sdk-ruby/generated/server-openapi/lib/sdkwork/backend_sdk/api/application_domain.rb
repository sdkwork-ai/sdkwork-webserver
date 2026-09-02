require_relative 'base_api'
require_relative '../models/applications_domains_create_response201'
require_relative '../models/applications_domains_list_response'
require_relative '../models/applications_domains_verify_response'
require_relative '../models/create_application_domain_request'

module Sdkwork
  module BackendSdk
    module Api
      class ApplicationDomainApi < BaseApi
          # List application domains
          def applications_domains_list(application_id, page: nil, page_size: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/domains', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsDomainsListResponse.from_hash(result) : nil
          end

          # Bind a public domain to an application
          def applications_domains_create(application_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/domains', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsDomainsCreateResponse201.from_hash(result) : nil
          end

          # Unbind an application public domain
          def applications_domains_delete(application_id, domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/domains/{domainId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
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

          # Create or check an application-domain ownership challenge
          def applications_domains_verify(application_id, domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/domains/{domainId}/verify', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsDomainsVerifyResponse.from_hash(result) : nil
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

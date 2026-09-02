require_relative 'base_api'
require_relative '../models/applications_domains_listener_certificate_bindings_create_response201'
require_relative '../models/applications_domains_listener_certificate_bindings_list_response'
require_relative '../models/create_listener_certificate_binding_request'

module Sdkwork
  module AppSdk
    module Api
      class CertificateApi < BaseApi
          # List certificates active on the domain listener
          def applications_domains_listener_certificate_bindings_list(application_id, domain_id, page: nil, page_size: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsDomainsListenerCertificateBindingsListResponse.from_hash(result) : nil
          end

          # Bind a certificate version to the domain listener
          def applications_domains_listener_certificate_bindings_create(application_id, domain_id, idempotency_key, body: nil)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::ApplicationsDomainsListenerCertificateBindingsCreateResponse201.from_hash(result) : nil
          end

          # Remove a certificate from the domain listener
          def applications_domains_listener_certificate_bindings_delete(application_id, domain_id, binding_id, idempotency_key)
            path = interpolate_path('/app/v3/api/applications/{applicationId}/domains/{domainId}/listener_certificate_bindings/{bindingId}', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)), domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)), bindingId: serialize_path_parameter(binding_id, PathParameterSpec.new('bindingId', 'simple', false)))
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

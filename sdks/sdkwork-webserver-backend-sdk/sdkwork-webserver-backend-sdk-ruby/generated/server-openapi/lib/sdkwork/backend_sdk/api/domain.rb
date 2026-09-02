require_relative 'base_api'
require_relative '../models/create_managed_domain_request'
require_relative '../models/create_root_domain_hostname_request'
require_relative '../models/create_root_domain_request'
require_relative '../models/domains_application_binding_update_response'
require_relative '../models/domains_create_response201'
require_relative '../models/domains_list_response'
require_relative '../models/domains_verify_response'
require_relative '../models/root_domains_create_response201'
require_relative '../models/root_domains_list_response'
require_relative '../models/root_domains_retrieve_response'
require_relative '../models/root_domains_subdomains_create_response201'
require_relative '../models/root_domains_subdomains_list_response'
require_relative '../models/update_domain_application_binding_request'

module Sdkwork
  module BackendSdk
    module Api
      class DomainApi < BaseApi
          # List tenant root-domain Zones
          def root_domains_list(page: nil, page_size: nil, status: nil, keyword: nil)
            path = '/backend/v3/api/root_domains'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
              QueryParameterSpec.new('keyword', keyword, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::RootDomainsListResponse.from_hash(result) : nil
          end

          # Define a tenant root-domain Zone
          def root_domains_create(idempotency_key, body: nil)
            path = '/backend/v3/api/root_domains'
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
            result.is_a?(Hash) ? Models::RootDomainsCreateResponse201.from_hash(result) : nil
          end

          # Retrieve a tenant root-domain Zone
          def root_domains_retrieve(root_domain_id)
            path = interpolate_path('/backend/v3/api/root_domains/{rootDomainId}', rootDomainId: serialize_path_parameter(root_domain_id, PathParameterSpec.new('rootDomainId', 'simple', false)))
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::RootDomainsRetrieveResponse.from_hash(result) : nil
          end

          # Delete an empty tenant root-domain Zone
          def root_domains_delete(root_domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/root_domains/{rootDomainId}', rootDomainId: serialize_path_parameter(root_domain_id, PathParameterSpec.new('rootDomainId', 'simple', false)))
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

          # List publishable hostnames in a root-domain Zone
          def root_domains_subdomains_list(root_domain_id, page: nil, page_size: nil)
            path = interpolate_path('/backend/v3/api/root_domains/{rootDomainId}/subdomains', rootDomainId: serialize_path_parameter(root_domain_id, PathParameterSpec.new('rootDomainId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::RootDomainsSubdomainsListResponse.from_hash(result) : nil
          end

          # Add a publishable hostname to a root-domain Zone
          def root_domains_subdomains_create(root_domain_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/root_domains/{rootDomainId}/subdomains', rootDomainId: serialize_path_parameter(root_domain_id, PathParameterSpec.new('rootDomainId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::RootDomainsSubdomainsCreateResponse201.from_hash(result) : nil
          end

          # List tenant custom domain assets
          def domains_list(page: nil, page_size: nil)
            path = '/backend/v3/api/domains'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::DomainsListResponse.from_hash(result) : nil
          end

          # Register a tenant custom domain asset
          def domains_create(idempotency_key, body: nil)
            path = '/backend/v3/api/domains'
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
            result.is_a?(Hash) ? Models::DomainsCreateResponse201.from_hash(result) : nil
          end

          # Delete an unbound tenant custom domain asset
          def domains_delete(domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/domains/{domainId}', domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
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

          # Create or check a tenant custom-domain ownership challenge
          def domains_verify(domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/domains/{domainId}/verify', domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
            request_headers = build_request_headers(
              {
                'Idempotency-Key' => HeaderParameterSpec.new(idempotency_key, 'simple', false, nil),
              },
              {}
            )
            options = {}
            options[:headers] = request_headers unless request_headers.empty?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::DomainsVerifyResponse.from_hash(result) : nil
          end

          # Bind a tenant custom domain to an application
          def domains_application_binding_update(domain_id, idempotency_key, body: nil)
            path = interpolate_path('/backend/v3/api/domains/{domainId}/application_binding', domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
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
            result.is_a?(Hash) ? Models::DomainsApplicationBindingUpdateResponse.from_hash(result) : nil
          end

          # Unbind a tenant custom domain from its application
          def domains_application_binding_delete(domain_id, idempotency_key)
            path = interpolate_path('/backend/v3/api/domains/{domainId}/application_binding', domainId: serialize_path_parameter(domain_id, PathParameterSpec.new('domainId', 'simple', false)))
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

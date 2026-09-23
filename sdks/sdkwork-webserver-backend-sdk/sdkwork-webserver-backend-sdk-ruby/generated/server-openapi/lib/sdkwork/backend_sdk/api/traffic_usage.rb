require_relative 'base_api'
require_relative '../models/platform_traffic_usages_retrieve_response'
require_relative '../models/traffic_usages_retrieve_response'

module Sdkwork
  module BackendSdk
    module Api
      class TrafficUsageApi < BaseApi
          # Retrieve aggregated traffic usage of the caller's own tenant
          def traffic_usages_retrieve(date_from: nil, date_to: nil, dimension: nil, top_apps: nil)
            path = '/backend/v3/api/traffic_usage'
            query = build_query_string([
              QueryParameterSpec.new('date_from', date_from, 'form', true, false, nil),
              QueryParameterSpec.new('date_to', date_to, 'form', true, false, nil),
              QueryParameterSpec.new('dimension', dimension, 'form', true, false, nil),
              QueryParameterSpec.new('top_apps', top_apps, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::TrafficUsagesRetrieveResponse.from_hash(result) : nil
          end

          # Retrieve aggregated traffic usage of every tenant
          def platform_traffic_usages_retrieve(date_from: nil, date_to: nil, dimension: nil, top_apps: nil)
            path = '/backend/v3/api/platform_traffic_usage'
            query = build_query_string([
              QueryParameterSpec.new('date_from', date_from, 'form', true, false, nil),
              QueryParameterSpec.new('date_to', date_to, 'form', true, false, nil),
              QueryParameterSpec.new('dimension', dimension, 'form', true, false, nil),
              QueryParameterSpec.new('top_apps', top_apps, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::PlatformTrafficUsagesRetrieveResponse.from_hash(result) : nil
          end

      end
    end
  end
end

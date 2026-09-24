require_relative 'base_api'
require_relative '../models/metrics_summaries_retrieve_response'
require_relative '../models/platform_metrics_summaries_retrieve_response'

module Sdkwork
  module BackendSdk
    module Api
      class MetricsSummaryApi < BaseApi
          # Retrieve the dashboard metric summary of the caller's own tenant
          def metrics_summaries_retrieve(date_from: nil, date_to: nil)
            path = '/backend/v3/api/metrics_summaries'
            query = build_query_string([
              QueryParameterSpec.new('date_from', date_from, 'form', true, false, nil),
              QueryParameterSpec.new('date_to', date_to, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::MetricsSummariesRetrieveResponse.from_hash(result) : nil
          end

          # Retrieve the dashboard metric summary of every tenant
          def platform_metrics_summaries_retrieve(date_from: nil, date_to: nil)
            path = '/backend/v3/api/platform_metrics_summaries'
            query = build_query_string([
              QueryParameterSpec.new('date_from', date_from, 'form', true, false, nil),
              QueryParameterSpec.new('date_to', date_to, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::PlatformMetricsSummariesRetrieveResponse.from_hash(result) : nil
          end

      end
    end
  end
end

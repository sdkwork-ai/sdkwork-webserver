require_relative 'base_api'
require_relative '../models/certificates_distribution_list_response'

module Sdkwork
  module BackendSdk
    module Api
      class CertificateDistributionApi < BaseApi
          # List certificate manifest convergence by server
          def certificates_distribution_list(page: nil, page_size: nil)
            path = '/backend/v3/api/certificate_distribution'
            query = build_query_string([
              QueryParameterSpec.new('page', page, 'form', true, false, nil),
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::CertificatesDistributionListResponse.from_hash(result) : nil
          end

      end
    end
  end
end

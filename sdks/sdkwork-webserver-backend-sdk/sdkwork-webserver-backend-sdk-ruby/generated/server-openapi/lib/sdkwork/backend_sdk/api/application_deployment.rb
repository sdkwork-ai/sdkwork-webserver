require_relative 'base_api'
require_relative '../models/applications_deployments_list_response'

module Sdkwork
  module BackendSdk
    module Api
      class ApplicationDeploymentApi < BaseApi
          # List application deployments
          def applications_deployments_list(application_id, page_size: nil, cursor: nil, status: nil)
            path = interpolate_path('/backend/v3/api/applications/{applicationId}/deployments', applicationId: serialize_path_parameter(application_id, PathParameterSpec.new('applicationId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
              QueryParameterSpec.new('status', status, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ApplicationsDeploymentsListResponse.from_hash(result) : nil
          end

      end
    end
  end
end

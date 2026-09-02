require_relative 'base_api'
require_relative '../models/audit_logs_list_response'

module Sdkwork
  module BackendSdk
    module Api
      class AuditApi < BaseApi
          # List audit logs
          def logs_list(page_size: nil, cursor: nil, target_type: nil, action: nil, operator_id: nil, start_date: nil, end_date: nil)
            path = '/backend/v3/api/audit_logs'
            query = build_query_string([
              QueryParameterSpec.new('page_size', page_size, 'form', true, false, nil),
              QueryParameterSpec.new('cursor', cursor, 'form', true, false, nil),
              QueryParameterSpec.new('target_type', target_type, 'form', true, false, nil),
              QueryParameterSpec.new('action', action, 'form', true, false, nil),
              QueryParameterSpec.new('operator_id', operator_id, 'form', true, false, nil),
              QueryParameterSpec.new('start_date', start_date, 'form', true, false, nil),
              QueryParameterSpec.new('end_date', end_date, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::AuditLogsListResponse.from_hash(result) : nil
          end

      end
    end
  end
end

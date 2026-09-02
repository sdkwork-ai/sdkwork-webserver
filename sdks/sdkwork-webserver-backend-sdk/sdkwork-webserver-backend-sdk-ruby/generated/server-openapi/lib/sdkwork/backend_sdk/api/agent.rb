require_relative 'base_api'
require_relative '../models/agent_heartbeat_request'
require_relative '../models/heartbeat_response'
require_relative '../models/retrieve_response'

module Sdkwork
  module BackendSdk
    module Api
      class AgentApi < BaseApi
          # Report an edge-agent heartbeat
          def heartbeat(body: nil)
            path = '/backend/v3/api/agent/heartbeat'
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            options = {}
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::HeartbeatResponse.from_hash(result) : nil
          end

          # Retrieve the Nginx configuration and certificate bundle
          def retrieve(if_sync_version: nil)
            path = '/backend/v3/api/agent/sync'
            query = build_query_string([
              QueryParameterSpec.new('if_sync_version', if_sync_version, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::RetrieveResponse.from_hash(result) : nil
          end

      end
    end
  end
end

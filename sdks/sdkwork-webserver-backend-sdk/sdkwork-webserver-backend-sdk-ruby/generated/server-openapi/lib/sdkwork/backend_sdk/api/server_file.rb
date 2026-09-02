require_relative 'base_api'
require_relative '../models/server_files_node_browse_response'
require_relative '../models/server_files_node_operations_create_response201'
require_relative '../models/server_files_node_operations_list_response'
require_relative '../models/server_files_node_read_response'
require_relative '../models/server_files_nodes_list_response'
require_relative '../models/server_run_operation_request'

module Sdkwork
  module BackendSdk
    module Api
      class ServerFileApi < BaseApi
          # List Server Files deployment nodes
          def server_files_nodes_list()
            path = '/backend/v3/api/server_files/nodes'
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ServerFilesNodesListResponse.from_hash(result) : nil
          end

          # Browse a deployment node directory
          def server_files_node_browse(node_id, path_)
            path = interpolate_path('/backend/v3/api/server_files/nodes/{nodeId}/browse', nodeId: serialize_path_parameter(node_id, PathParameterSpec.new('nodeId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('path', path_, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ServerFilesNodeBrowseResponse.from_hash(result) : nil
          end

          # Read a text file on a deployment node
          def server_files_node_read(node_id, path_)
            path = interpolate_path('/backend/v3/api/server_files/nodes/{nodeId}/read', nodeId: serialize_path_parameter(node_id, PathParameterSpec.new('nodeId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('path', path_, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ServerFilesNodeReadResponse.from_hash(result) : nil
          end

          # List operations available for a project directory
          def server_files_node_operations_list(node_id, path_)
            path = interpolate_path('/backend/v3/api/server_files/nodes/{nodeId}/operations', nodeId: serialize_path_parameter(node_id, PathParameterSpec.new('nodeId', 'simple', false)))
            query = build_query_string([
              QueryParameterSpec.new('path', path_, 'form', true, false, nil),
            ])
            path = append_query_string(path, query)
            options = {}

            result = @client.request('GET', path, **options)
            result.is_a?(Hash) ? Models::ServerFilesNodeOperationsListResponse.from_hash(result) : nil
          end

          # Run a project operation on a deployment node
          def server_files_node_operations_create(node_id, body: nil)
            path = interpolate_path('/backend/v3/api/server_files/nodes/{nodeId}/operations', nodeId: serialize_path_parameter(node_id, PathParameterSpec.new('nodeId', 'simple', false)))
            payload = body.respond_to?(:to_hash) ? body.to_hash : body
            options = {}
            options[:json] = payload unless payload.nil?
            result = @client.request('POST', path, **options)
            result.is_a?(Hash) ? Models::ServerFilesNodeOperationsCreateResponse201.from_hash(result) : nil
          end

      end
    end
  end
end

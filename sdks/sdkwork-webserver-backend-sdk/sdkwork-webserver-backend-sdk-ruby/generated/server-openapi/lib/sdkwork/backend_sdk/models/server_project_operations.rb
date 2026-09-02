module Sdkwork
  module BackendSdk
    module Models
      class ServerProjectOperations
              attr_accessor :node_id, :path, :project_type, :operations

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @node_id = attributes['nodeId']
                @path = attributes['path']
                @project_type = attributes['projectType']
                @operations = attributes['operations'].is_a?(Array) ? attributes['operations'].map { |item| item.is_a?(Hash) ? ServerProjectOperation.from_hash(item) : item } : []
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'nodeId' => @node_id,
                  'path' => @path,
                  'projectType' => @project_type,
                  'operations' => @operations.is_a?(Array) ? @operations.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                }
              end
            end
    end
  end
end

module Sdkwork
  module BackendSdk
    module Models
      class ServerDirectoryListing
              attr_accessor :node_id, :path, :parent_path, :entries, :truncated

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @node_id = attributes['nodeId']
                @path = attributes['path']
                @parent_path = attributes['parentPath']
                @entries = attributes['entries'].is_a?(Array) ? attributes['entries'].map { |item| item.is_a?(Hash) ? ServerEntry.from_hash(item) : item } : []
                @truncated = attributes['truncated']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'nodeId' => @node_id,
                  'path' => @path,
                  'parentPath' => @parent_path,
                  'entries' => @entries.is_a?(Array) ? @entries.map { |item| item.respond_to?(:to_hash) ? item.to_hash : item } : [],
                  'truncated' => @truncated,
                }
              end
            end
    end
  end
end

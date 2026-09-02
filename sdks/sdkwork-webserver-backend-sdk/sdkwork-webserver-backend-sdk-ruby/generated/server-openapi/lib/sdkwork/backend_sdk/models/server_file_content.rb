module Sdkwork
  module BackendSdk
    module Models
      class ServerFileContent
              attr_accessor :node_id, :path, :content, :size

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @node_id = attributes['nodeId']
                @path = attributes['path']
                @content = attributes['content']
                @size = attributes['size']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'nodeId' => @node_id,
                  'path' => @path,
                  'content' => @content,
                  'size' => @size,
                }
              end
            end
    end
  end
end

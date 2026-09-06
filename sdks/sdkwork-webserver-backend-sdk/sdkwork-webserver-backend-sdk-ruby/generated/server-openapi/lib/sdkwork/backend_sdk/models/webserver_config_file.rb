module Sdkwork
  module BackendSdk
    module Models
      class WebserverConfigFile
              attr_accessor :id, :kind, :name, :path, :language, :writable, :content, :size, :sha256, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @kind = attributes['kind']
                @name = attributes['name']
                @path = attributes['path']
                @language = attributes['language']
                @writable = attributes['writable']
                @content = attributes['content']
                @size = attributes['size']
                @sha256 = attributes['sha256']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'kind' => @kind,
                  'name' => @name,
                  'path' => @path,
                  'language' => @language,
                  'writable' => @writable,
                  'content' => @content,
                  'size' => @size,
                  'sha256' => @sha256,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

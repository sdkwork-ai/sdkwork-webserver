module Sdkwork
  module BackendSdk
    module Models
      class WebserverConfigEntry
              attr_accessor :id, :kind, :name, :path, :language, :size, :updated_at, :writable

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @kind = attributes['kind']
                @name = attributes['name']
                @path = attributes['path']
                @language = attributes['language']
                @size = attributes['size']
                @updated_at = attributes['updatedAt']
                @writable = attributes['writable']
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
                  'size' => @size,
                  'updatedAt' => @updated_at,
                  'writable' => @writable,
                }
              end
            end
    end
  end
end

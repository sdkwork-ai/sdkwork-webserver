module Sdkwork
  module BackendSdk
    module Models
      class WebserverConfigWriteResult
              attr_accessor :id, :path, :size, :sha256, :backup_path, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @path = attributes['path']
                @size = attributes['size']
                @sha256 = attributes['sha256']
                @backup_path = attributes['backupPath']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'path' => @path,
                  'size' => @size,
                  'sha256' => @sha256,
                  'backupPath' => @backup_path,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

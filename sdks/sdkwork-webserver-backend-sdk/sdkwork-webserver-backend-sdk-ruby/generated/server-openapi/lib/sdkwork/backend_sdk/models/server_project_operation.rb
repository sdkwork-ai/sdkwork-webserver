module Sdkwork
  module BackendSdk
    module Models
      class ServerProjectOperation
              attr_accessor :id, :kind, :label, :permission, :description, :dangerous

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @kind = attributes['kind']
                @label = attributes['label']
                @permission = attributes['permission']
                @description = attributes['description']
                @dangerous = attributes['dangerous']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'kind' => @kind,
                  'label' => @label,
                  'permission' => @permission,
                  'description' => @description,
                  'dangerous' => @dangerous,
                }
              end
            end
    end
  end
end

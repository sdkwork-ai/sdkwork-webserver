module Sdkwork
  module AppSdk
    module Models
      class SdkWorkCommandData
              attr_accessor :accepted, :resource_id, :status

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @accepted = attributes['accepted']
                @resource_id = attributes['resourceId']
                @status = attributes['status']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'accepted' => @accepted,
                  'resourceId' => @resource_id,
                  'status' => @status,
                }
              end
            end
    end
  end
end

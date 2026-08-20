module Sdkwork
  module AppSdk
    module Models
      class PlatformTargetResponse
              attr_accessor :id, :app_id, :target_key, :platform, :tech_stack, :architectures, :bundle_id, :package_name, :app_id_value, :bundle_name, :target_status, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @app_id = attributes['appId']
                @target_key = attributes['targetKey']
                @platform = attributes['platform']
                @tech_stack = attributes['techStack']
                @architectures = attributes['architectures'].is_a?(Array) ? attributes['architectures'].map { |item| item } : []
                @bundle_id = attributes['bundleId']
                @package_name = attributes['packageName']
                @app_id_value = attributes['appIdValue']
                @bundle_name = attributes['bundleName']
                @target_status = attributes['targetStatus']
                @created_at = attributes['createdAt']
                @updated_at = attributes['updatedAt']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'appId' => @app_id,
                  'targetKey' => @target_key,
                  'platform' => @platform,
                  'techStack' => @tech_stack,
                  'architectures' => @architectures.is_a?(Array) ? @architectures.map { |item| item } : [],
                  'bundleId' => @bundle_id,
                  'packageName' => @package_name,
                  'appIdValue' => @app_id_value,
                  'bundleName' => @bundle_name,
                  'targetStatus' => @target_status,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

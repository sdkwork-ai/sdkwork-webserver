module Sdkwork
  module BackendSdk
    module Models
      class ClusterHostResponse
              attr_accessor :id, :cluster_id, :name, :hostname, :machine_code, :os_name, :os_version, :kernel_version, :arch, :cpu_model, :cpu_cores, :memory_total_mb, :remote_ip, :local_ips, :mac_addresses, :daemon_version, :status, :last_heartbeat_at, :instance_count, :created_at, :updated_at

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @cluster_id = attributes['clusterId']
                @name = attributes['name']
                @hostname = attributes['hostname']
                @machine_code = attributes['machineCode']
                @os_name = attributes['osName']
                @os_version = attributes['osVersion']
                @kernel_version = attributes['kernelVersion']
                @arch = attributes['arch']
                @cpu_model = attributes['cpuModel']
                @cpu_cores = attributes['cpuCores']
                @memory_total_mb = attributes['memoryTotalMb']
                @remote_ip = attributes['remoteIp']
                @local_ips = attributes['localIps'].is_a?(Array) ? attributes['localIps'].map { |item| item } : []
                @mac_addresses = attributes['macAddresses'].is_a?(Array) ? attributes['macAddresses'].map { |item| item } : []
                @daemon_version = attributes['daemonVersion']
                @status = attributes['status']
                @last_heartbeat_at = attributes['lastHeartbeatAt']
                @instance_count = attributes['instanceCount']
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
                  'clusterId' => @cluster_id,
                  'name' => @name,
                  'hostname' => @hostname,
                  'machineCode' => @machine_code,
                  'osName' => @os_name,
                  'osVersion' => @os_version,
                  'kernelVersion' => @kernel_version,
                  'arch' => @arch,
                  'cpuModel' => @cpu_model,
                  'cpuCores' => @cpu_cores,
                  'memoryTotalMb' => @memory_total_mb,
                  'remoteIp' => @remote_ip,
                  'localIps' => @local_ips.is_a?(Array) ? @local_ips.map { |item| item } : [],
                  'macAddresses' => @mac_addresses.is_a?(Array) ? @mac_addresses.map { |item| item } : [],
                  'daemonVersion' => @daemon_version,
                  'status' => @status,
                  'lastHeartbeatAt' => @last_heartbeat_at,
                  'instanceCount' => @instance_count,
                  'createdAt' => @created_at,
                  'updatedAt' => @updated_at,
                }
              end
            end
    end
  end
end

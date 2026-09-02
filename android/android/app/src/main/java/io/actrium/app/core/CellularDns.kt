package io.actrium.app.core

import okhttp3.Dns
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress

class CellularDns : Dns {

    override fun lookup(hostname: String): List<InetAddress> {
        val addresses = InetAddress.getAllByName(hostname)
        try {
            val ipv4Addresses = mutableListOf<InetAddress>()
            val ipv6Addresses = mutableListOf<InetAddress>()
            for (address in addresses) {
                if (address is Inet4Address) {
                    ipv4Addresses.add(address)
                } else if (address is Inet6Address) {
                    ipv6Addresses.add(address)
                }
            }
            // Return Priority IPv4 Address
            return ipv4Addresses + ipv6Addresses
        } catch (e: NullPointerException) {
            e.printStackTrace()
        }
        return addresses.toList()
    }
}
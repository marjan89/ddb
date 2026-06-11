package io.substrate.regdemo.xml

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView

class T25Activity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_t25)
        val rv = findViewById<RecyclerView>(R.id.t25_list)
        rv.layoutManager = LinearLayoutManager(this)
        rv.adapter = T25Adapter((1..30).map { String.format("T25 Item %02d", it) })
    }
}

class T25Adapter(private val items: List<String>) : RecyclerView.Adapter<T25Adapter.VH>() {
    class VH(val tv: TextView) : RecyclerView.ViewHolder(tv)
    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH {
        val tv = LayoutInflater.from(parent.context)
            .inflate(android.R.layout.simple_list_item_1, parent, false) as TextView
        return VH(tv)
    }
    override fun onBindViewHolder(holder: VH, position: Int) {
        holder.tv.text = items[position]
        holder.tv.contentDescription = items[position]
    }
    override fun getItemCount(): Int = items.size
}
